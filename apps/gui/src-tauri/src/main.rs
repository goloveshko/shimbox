//! shimbox manager: the desktop UI that generates shims. Windows only.
//! See docs/techspec.md 5.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pe;
mod runner;
mod shim;

use std::path::Path;
use std::process::Command;
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
        .invoke_handler(tauri::generate_handler![
            detect_variant,
            embedded_runners,
            pick_file,
            pick_folder,
            open_folder,
            shim::generate
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

#[tauri::command]
async fn pick_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    // The dialog runs on its own thread; blocking the async worker is not
    // enough to keep it off the event loop.
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("executables", &["exe"])
            .blocking_pick_file()
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(picked.map(|file| file.to_string()))
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().blocking_pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(picked.map(|file| file.to_string()))
}

/// The success panel's "Open destination folder" action (5.1).
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
