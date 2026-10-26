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
/// The bytes land in a file beside the real one and arrive by a rename, so what
/// a person holds is either the whole previous setting or the whole new one: a
/// start stopped mid-write leaves the old file standing, and the reading rule
/// above never has to meet a half-written JSON.
pub fn write<T: serde::Serialize>(file: &Path, value: &T) {
    let Ok(text) = serde_json::to_string_pretty(value) else {
        return;
    };
    let Some(directory) = file.parent() else {
        return;
    };
    let Ok(mut temp) = file.as_os_str().to_os_string().into_string() else {
        return;
    };
    temp.push_str(".tmp");
    let temp = PathBuf::from(temp);
    if fs::create_dir_all(directory).is_ok() && fs::write(&temp, text).is_ok() {
        let _ = fs::rename(&temp, file);
    }
}

/// Where this app keeps the file called `name`, or nothing when the system will
/// not say where its own config folder is.
pub fn file(app: &tauri::AppHandle, name: &str) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct Theme(String);

    #[test]
    fn a_saved_setting_reads_back_and_leaves_no_temporary_behind() {
        let dir = crate::testutil::scratch("state", "write");
        let file = dir.join("theme.json");

        write(&file, &Theme("dark".to_string()));
        assert_eq!(read::<Theme>(&file).0, "dark");

        // The rename took the temporary with it; one left beside the real file is
        // a setting nobody can read.
        let names: HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, HashSet::from(["theme.json".to_string()]));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_cannot_be_understood_is_an_empty_one() {
        let dir = crate::testutil::scratch("state", "read");
        let file = dir.join("theme.json");
        fs::write(&file, b"{ \"name\": ").unwrap();

        assert!(read::<Theme>(&file).0.is_empty());

        fs::remove_dir_all(&dir).ok();
    }
}
