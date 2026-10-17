//! The small files the app keeps in `$APPCONFIG` beside the window's own
//! geometry: the theme and the language, the folder each Browse button last
//! looked in, the moment a release was last asked about. None of the three is
//! worth an error path of its own, so they share how they are read and written
//! and each keeps only what makes it itself: a name and a shape
//! (docs/techspec.md 5).

use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// A file that cannot be read or understood is an empty one. These are
/// conveniences, and losing one cannot be allowed to stop the window from
/// drawing, a dialog from opening or a start from finishing.
pub fn read<T: Default + serde::de::DeserializeOwned>(file: &Path) -> T {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The whole value at once: every one of these files holds a handful of keys, so
/// rewriting it costs nothing and keeps the reading rule to the one above.
pub fn write<T: serde::Serialize>(file: &Path, value: &T) {
    let Ok(text) = serde_json::to_string_pretty(value) else {
        return;
    };
    let Some(directory) = file.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_ok() {
        let _ = fs::write(file, text);
    }
}

/// Where this app keeps the file called `name`, or nothing when the system will
/// not say where its own config folder is.
pub fn file(app: &tauri::AppHandle, name: &str) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(name))
}
