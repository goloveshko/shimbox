//! shimbox manager: the desktop UI that generates shims. Windows only.
//! See docs/techspec.md 5.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod batch;
mod pe;
mod pickers;
mod runner;
mod secret;
mod settings;
mod shim;

use std::path::Path;
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
            batch::generate_batch
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
        let dialog = dialog_app.dialog().file();
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
        Some(start) => dialog_app.dialog().file().set_directory(start).blocking_pick_folder(),
        None => dialog_app.dialog().file().blocking_pick_folder(),
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(picked.map(|folder| {
        let picked = folder.to_string();
        pickers::remember(&app, &key, Path::new(&picked));
        picked
    }))
}

/// The success dialog's "Open destination folder" action (5.1).
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    // `explorer` reports a non-zero exit code even when it opened the folder,
    // so its status says nothing and is ignored.
    Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|error| format!("cannot open {path}: {error}"))
        .map(|_| ())
}

/// The About card's "settings folder" row: the folder the app's own files live
/// in, which is what a report of a strange build starts by asking for.
#[tauri::command]
fn app_folder(app: tauri::AppHandle) -> Option<String> {
    app.path().app_config_dir().ok().map(|dir| dir.display().to_string())
}

/// The About card's links (5): an address is handed to the browser rather than
/// navigated to, so the window keeps showing the app.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    // The two pages the card names, and nothing else: a command that opens any
    // string the page holds is a way to point the browser anywhere, which the
    // card has no use for.
    const HOSTS: [&str; 2] = ["github.com", "sergey.is-a.dev"];
    let host = url
        .strip_prefix("https://")
        .unwrap_or_default()
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if !HOSTS.contains(&host) {
        return Err(format!("refusing to open {url}"));
    }
    // `explorer` takes an address the way it takes a folder and hands it to the
    // default browser, and reports a non-zero exit code either way.
    Command::new("explorer")
        .arg(&url)
        .spawn()
        .map_err(|error| format!("cannot open {url}: {error}"))
        .map(|_| ())
}

/// The success dialog's "Edit PATH…" action. A folder becomes a command name
/// through the system's own variable editor, so the user types the entry and
/// shimbox never writes to the registry (6).
#[tauri::command]
fn edit_path() -> Result<(), String> {
    // One argument, not two: `dll,entry` is rundll32's own syntax. Both it and
    // the editor it loads are GUI binaries, so no console flashes up behind.
    Command::new("rundll32.exe")
        .arg("sysdm.cpl,EditEnvironmentVariables")
        .spawn()
        .map_err(|error| format!("cannot open the environment editor: {error}"))
        .map(|_| ())
}
