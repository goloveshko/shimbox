//! The choices made in the Settings dialog (docs/techspec.md 5). They live in
//! `$APPCONFIG/.settings.json`, beside the window's geometry and the pickers'
//! folders, so a theme survives a restart and can be read without the app
//! running.

use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const FILE: &str = ".settings.json";

/// The values the dialog offers. Anything else in the file is not a choice the
/// app understands, so it reads as no choice at all.
const THEMES: [&str; 3] = ["system", "light", "dark"];

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Settings {
    theme: Option<String>,
}

impl Settings {
    /// A theme only counts when it is one of the offered names; an absent file,
    /// an absent key and a hand-written nonsense value all mean the same thing,
    /// which is that the app has never been asked.
    fn theme(&self) -> Option<&'static str> {
        self.theme
            .as_deref()
            .and_then(|theme| THEMES.into_iter().find(|offered| *offered == theme))
    }
}

/// A file that cannot be read or understood is an empty one: this is a
/// convenience, and losing it cannot be allowed to stop the window from drawing.
fn read(file: &Path) -> Settings {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(file: &Path, settings: &Settings) {
    let Ok(text) = serde_json::to_string_pretty(settings) else {
        return;
    };
    let Some(directory) = file.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_ok() {
        let _ = fs::write(file, text);
    }
}

fn state_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(FILE))
}

#[tauri::command]
pub fn get_theme(app: tauri::AppHandle) -> Option<String> {
    let file = state_file(&app)?;
    Some(read(&file).theme()?.to_string())
}

#[tauri::command]
pub fn set_theme(app: tauri::AppHandle, theme: String) -> Option<String> {
    let file = state_file(&app)?;
    let settings = Settings {
        theme: Some(theme),
    };
    write(&file, &settings);
    settings.theme().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shimbox-settings-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_written_choice_is_the_one_read_back() {
        let root = scratch("round-trip");
        let file = root.join(FILE);

        assert_eq!(read(&file).theme(), None, "no file is no choice");

        write(&file, &Settings { theme: Some("dark".to_string()) });
        assert_eq!(read(&file).theme(), Some("dark"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_value_the_app_does_not_offer_is_no_value() {
        let root = scratch("unknown");
        let file = root.join(FILE);

        fs::write(&file, r#"{"theme":"neon"}"#).unwrap();
        assert_eq!(read(&file).theme(), None);

        fs::write(&file, r#"{"theme":null}"#).unwrap();
        assert_eq!(read(&file).theme(), None);

        fs::write(&file, "not json at all").unwrap();
        assert_eq!(read(&file).theme(), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_key_the_app_does_not_know_leaves_the_theme_alone() {
        let root = scratch("extra-key");
        let file = root.join(FILE);

        // Written by a newer build: the theme it carries is still the one to use.
        fs::write(&file, r#"{"theme":"light","language":"ru"}"#).unwrap();
        assert_eq!(read(&file).theme(), Some("light"));

        fs::remove_dir_all(&root).ok();
    }
}
