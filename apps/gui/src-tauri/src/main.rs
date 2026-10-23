//! shimbox manager: the desktop UI that generates shims. Windows only.
//! See docs/techspec.md 5.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod batch;
mod pe;
mod pickers;
mod refresh;
mod runner;
mod secret;
mod settings;
mod shim;
mod state;
#[cfg(test)]
mod testutil;
mod update;

use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use runner::Variant;

/// Reported to the UI so the `Auto detect` choice is visible before generating
/// (docs/techspec.md 5.4).
#[derive(serde::Serialize)]
struct Detected {
    variant: &'static str,
    subsystem: &'static str,
}

#[derive(serde::Serialize)]
struct EmbeddedRunner {
    variant: &'static str,
    size: usize,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // The size and the corner the window was left in are the user's, not the
        // config file's, so they are written back on close and read again on start.
        // The window starts hidden (`visible: false`) because this plugin is what
        // shows it, already at the saved geometry, instead of letting it flash at
        // the configured one first. Deleting `$APPCONFIG/.window-state.json` is the
        // reset; a first run has no state and gets the size from the config.
        // Decorations are deliberately not restored: the caption is drawn by the UI
        // now, and a state file written while the window still had the native one
        // would put that titlebar back above ours.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::DECORATIONS,
                )
                .build(),
        )
        .plugin(
            // The state plugin restores the size while the window is still being
            // created, before Windows drops the caption it keeps under an undecorated
            // window's shadow, so that height is measured against a hidden caption:
            // the window opens a caption taller, saves that size and grows again on
            // every start. Asked one step later the same size lands exactly, and this
            // still runs before the first paint. A maximized or fullscreen window has
            // no size of its own to re-apply and giving it one would drop it back.
            tauri::plugin::Builder::<tauri::Wry>::new("remembered-size")
                .on_window_ready(|window| {
                    use tauri_plugin_window_state::WindowExt;
                    if window.label() != "main"
                        || window.is_maximized().unwrap_or_default()
                        || window.is_fullscreen().unwrap_or_default()
                    {
                        return;
                    }
                    let _ = window.restore_state(tauri_plugin_window_state::StateFlags::SIZE);
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            detect_variant,
            embedded_runners,
            pick_file,
            pick_folder,
            open_folder,
            open_link,
            app_folder,
            folder_on_path,
            paths_are_folders,
            edit_path,
            secret::store_secret,
            secret::forget_secret,
            secret::list_secrets,
            settings::get_theme,
            settings::set_theme,
            settings::get_language,
            settings::set_language,
            shim::generate,
            batch::scan,
            batch::generate_batch,
            refresh::scan_shims,
            refresh::refresh_runners,
            update::update_due,
            update::update_seen
        ])
        .run(tauri::generate_context!())
        .expect("shimbox failed to start");
}

#[tauri::command]
fn detect_variant(path: String) -> Detected {
    let subsystem = pe::subsystem_of(Path::new(&path));
    // A stable token, not prose: the UI picks the sentence for it.
    let name = match subsystem {
        pe::Subsystem::Gui => "gui",
        pe::Subsystem::Console => "console",
        pe::Subsystem::Unknown => "unknown",
    };
    Detected {
        variant: Variant::from(subsystem).name(),
        subsystem: name,
    }
}

#[tauri::command]
fn embedded_runners() -> Vec<EmbeddedRunner> {
    vec![
        EmbeddedRunner {
            variant: "cli",
            size: runner::Variant::Cli.bytes().len(),
        },
        EmbeddedRunner {
            variant: "gui",
            size: runner::Variant::Gui.bytes().len(),
        },
    ]
}

/// The dialog both pickers open, owned by the main window. The owner is what
/// keeps the dialog off the taskbar: without one the shell gives it a button of
/// its own, and a thumbnail wearing the generic icon rather than ours.
fn file_dialog(app: &tauri::AppHandle) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    let dialog = app.dialog().file();
    match app.get_webview_window("main") {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

/// Both pickers start where their own field last looked rather than where the
/// previous field's dialog ended, and remember it for the next time (5).
#[tauri::command]
async fn pick_file(
    app: tauri::AppHandle,
    key: String,
    path: String,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let start = pickers::start_directory(&app, &key, &path);
    // Which files the dialog offers belongs to the field, not to the command:
    // the target is an executable, while the file a variable reads its value
    // from is a text file that the same filter would hide.
    let only_executables = key == "target";
    // The dialog runs on its own thread; blocking the async worker is not
    // enough to keep it off the event loop.
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = file_dialog(&dialog_app);
        let dialog = if only_executables {
            dialog.add_filter("executables", &["exe"])
        } else {
            dialog
        };
        match start {
            Some(start) => dialog.set_directory(start).blocking_pick_file(),
            None => dialog.blocking_pick_file(),
        }
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(picked.map(|file| {
        let picked = file.to_string();
        pickers::remember(&app, &key, Path::new(&picked));
        picked
    }))
}

#[tauri::command]
async fn pick_folder(
    app: tauri::AppHandle,
    key: String,
    path: String,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let start = pickers::start_directory(&app, &key, &path);
    let picked = tauri::async_runtime::spawn_blocking(move || match start {
        Some(start) => file_dialog(&dialog_app)
            .set_directory(start)
            .blocking_pick_folder(),
        None => file_dialog(&dialog_app).blocking_pick_folder(),
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(picked.map(|folder| {
        let picked = folder.to_string();
        pickers::remember(&app, &key, Path::new(&picked));
        picked
    }))
}

/// A binary the system owns, named from the directory the environment says Windows
/// lives in. A bare name is answered by the search order, and that order tries the
/// folder the manager was started from before it tries the system's own - which for
/// a tool whose whole job is a folder on `PATH` is a folder of shims.
fn system_binary(relative: &str) -> Command {
    let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"));
    // A system that says nothing about where it lives still has the search order to
    // answer with, so the bare name is the fallback rather than a refusal to run.
    Command::new(match root {
        Some(root) => PathBuf::from(root).join(relative),
        None => PathBuf::from(relative),
    })
}

/// The shell, which opens a folder and hands an address to the default browser.
fn explorer() -> Command {
    system_binary("explorer.exe")
}

/// The loader the system's own environment editor is reached through (6).
fn rundll32() -> Command {
    system_binary(r"System32\rundll32.exe")
}

/// The success dialog's "Open destination folder" action (5.1).
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    // `explorer` reports a non-zero exit code even when it opened the folder,
    // so its status says nothing and is ignored.
    explorer()
        .arg(&path)
        .spawn()
        .map_err(|error| format!("cannot open {path}: {error}"))
        .map(|_| ())
}

/// The About card's "settings folder" row: the folder the app's own files live
/// in, which is what a report of a strange build starts by asking for.
#[tauri::command]
fn app_folder(app: tauri::AppHandle) -> Option<String> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.display().to_string())
}

/// Whether an address is one of the two pages the About card names, and nothing
/// else. The URL is read by the same parser a browser uses rather than cut apart
/// here: the host a hand-picked substring names and the host the browser resolves
/// are two answers, and only the second one opens a page. `https://github.com:443@
/// evil.example` is the shape that got away from a substring.
fn link_is_ours(url: &str) -> bool {
    const HOSTS: [&str; 2] = ["github.com", "sergey.is-a.dev"];
    if url.chars().any(|c| c.is_whitespace() || c < '\u{20}') {
        return false;
    }
    let Ok(address) = url::Url::parse(url) else {
        return false;
    };
    // An https address of one of the two hosts, with no authority in front of it
    // and no port other than the one the scheme already means.
    address.username().is_empty()
        && address.password().is_none()
        && address.port_or_known_default() == Some(443)
        && address
            .host_str()
            .is_some_and(|host| HOSTS.iter().any(|name| name.eq_ignore_ascii_case(host)))
}

/// The About card's links (5): an address is handed to the browser rather than
/// navigated to, so the window keeps showing the app.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !link_is_ours(&url) {
        return Err(format!("refusing to open {url}"));
    }
    // `explorer` takes an address the way it takes a folder and hands it to the
    // default browser, and reports a non-zero exit code either way.
    explorer()
        .arg(&url)
        .spawn()
        .map_err(|error| format!("cannot open {url}: {error}"))
        .map(|_| ())
}

/// Whether a folder is one of the entries of a `PATH`-shaped list. Windows looks
/// a command name up in those entries in order, and an empty one means the current
/// directory rather than a folder, so it answers no question about a destination.
fn contains_folder(path_list: &std::ffi::OsStr, folder: &Path) -> bool {
    std::env::split_paths(path_list)
        .any(|entry| !entry.as_os_str().is_empty() && shim::same_path(&entry, folder))
}

/// The question behind the success dialog's reminder: can this folder already be
/// reached by name? The answer is read from the variable this process was started
/// with rather than from the registry, so the app asks nothing of Windows about an
/// entry it only reports. The cost of that is a folder added while the window
/// stayed open: it is not seen here, and the reminder is shown once more than the
/// work needed it.
#[tauri::command]
fn folder_on_path(path: String) -> bool {
    let variable = std::env::var_os("Path").unwrap_or_default();
    contains_folder(&variable, Path::new(&path))
}

/// What the window does with a dropped path depends on whether it names a folder,
/// and the shape of a name proves nothing: a folder can end in `.exe` and a
/// program needs no extension at all. The page cannot ask the file system, so the
/// answers come back in the order the paths were sent.
#[tauri::command]
fn paths_are_folders(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|path| Path::new(path).is_dir()).collect()
}

/// The success dialog's "Edit PATH…" action. A folder becomes a command name
/// through the system's own variable editor, so the user types the entry and
/// shimbox never writes to the registry (6).
#[tauri::command]
fn edit_path() -> Result<(), String> {
    // One argument, not two: `dll,entry` is rundll32's own syntax. Both it and
    // the editor it loads are GUI binaries, so no console flashes up behind.
    rundll32()
        .arg("sysdm.cpl,EditEnvironmentVariables")
        .spawn()
        .map_err(|error| format!("cannot open the environment editor: {error}"))
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::contains_folder;
    use std::path::Path;

    #[test]
    fn a_folder_is_found_whatever_the_way_it_is_written() {
        let variable = "C:\\Windows;C:\\shimbox\\bin;D:\\Other";
        let folder = Path::new("C:\\shimbox\\bin");
        assert!(contains_folder(variable.as_ref(), folder));
        // Windows ignores the case of a name and a separator that says nothing.
        assert!(contains_folder(
            "c:\\SHIMBOX\\BIN\\".as_ref(),
            Path::new("C:\\shimbox\\bin")
        ));
        assert!(!contains_folder(
            variable.as_ref(),
            Path::new("C:\\shimbox")
        ));
    }

    #[test]
    fn an_empty_entry_names_no_folder() {
        // A doubled separator leaves an empty entry, which to Windows is the
        // current directory; it is no answer about a destination.
        let variable = ";;C:\\Windows";
        assert!(contains_folder(variable.as_ref(), Path::new("C:\\Windows")));
        assert!(!contains_folder(variable.as_ref(), Path::new("")));
        assert!(!contains_folder("".as_ref(), Path::new("C:\\shimbox\\bin")));
    }

    #[test]
    fn only_the_two_pages_of_the_card_reach_a_browser() {
        for url in [
            "https://github.com",
            "https://github.com/goloveshko/shimbox/releases/latest",
            "https://sergey.is-a.dev/",
            // One slash is a thing a person mis-types and a browser opens anyway,
            // reading it as the page above: the answer here follows the browser,
            // which is the one that decides where to go.
            "https:/github.com",
        ] {
            assert!(super::link_is_ours(url), "refused {url}");
        }

        for url in [
            // The host is the name in front of the path, so a port and a userinfo
            // written where the host should be do not move it anywhere: a browser
            // reads `evil.example` here, which is the one answer a substring check
            // could not see.
            "https://github.com:443@evil.example/release",
            "https://evil.example/#https://github.com",
            "https://github.com.evil.example/",
            "https://github.com:8443/",
            "http://github.com/",
            "file:///C:/Windows/system32",
            "https://github.com\u{0}@evil.example",
            "javascript:alert(1)",
        ] {
            assert!(!super::link_is_ours(url), "accepted {url}");
        }
    }

    #[test]
    fn a_system_binary_is_named_by_the_folder_the_system_lives_in() {
        // The point of the helper is that the search order never gets to answer, so
        // what the command carries is a path rather than a name to look up.
        let root = std::env::var_os("SystemRoot").expect("Windows says where it lives");
        assert_eq!(
            super::explorer().get_program(),
            Path::new(&root).join("explorer.exe").as_os_str()
        );
        assert_eq!(
            super::rundll32().get_program(),
            Path::new(&root).join(r"System32\rundll32.exe").as_os_str()
        );
    }
}
