//! The choices made in the Settings dialog (docs/techspec.md 5). They live in
//! `$APPCONFIG/.settings.json`, beside the window's geometry and the pickers'
//! folders, so a theme or a language survives a restart and can be read without
//! the app running.

use std::path::{Path, PathBuf};

use crate::state;

const FILE: &str = ".settings.json";

/// The values the dialog offers. Anything else in the file is not a choice the
/// app understands, so it reads as no choice at all.
const THEMES: [&str; 3] = ["system", "light", "dark"];
/// The codes `ui/i18n.js` registers a dictionary under.
const LANGUAGES: [&str; 2] = ["en", "ru"];

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Settings {
    // A question the dialog never asked is left out of the file rather than
    // written as a null, so a file with one choice in it reads as one choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

impl Settings {
    /// A choice only counts when it is one of the offered names; an absent file,
    /// an absent key and a hand-written nonsense value all mean the same thing,
    /// which is that the app has never been asked.
    fn theme(&self) -> Option<&'static str> {
        offered(&THEMES, self.theme.as_deref())
    }

    fn language(&self) -> Option<&'static str> {
        offered(&LANGUAGES, self.language.as_deref())
    }
}

fn offered(names: &'static [&'static str], value: Option<&str>) -> Option<&'static str> {
    value.and_then(|value| names.iter().copied().find(|name| *name == value))
}

// The rule in `state`, bound to this file's name and its shape: a file that
// cannot be read or understood is an empty one, and losing it cannot be allowed
// to stop the window from drawing.
fn read(file: &Path) -> Settings {
    state::read(file)
}

fn write(file: &Path, settings: &Settings) {
    state::write(file, settings);
}

fn state_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    state::file(app, FILE)
}

#[tauri::command]
pub fn get_theme(app: tauri::AppHandle) -> Option<String> {
    let file = state_file(&app)?;
    Some(read(&file).theme()?.to_string())
}

#[tauri::command]
pub fn get_language(app: tauri::AppHandle) -> Option<String> {
    let file = state_file(&app)?;
    Some(read(&file).language()?.to_string())
}

/// Both setters change the file they read rather than replace it: answering one
/// of the dialog's questions must not unask the other.
fn edit(file: &Path, change: impl FnOnce(&mut Settings)) -> Settings {
    let mut settings = read(file);
    change(&mut settings);
    write(file, &settings);
    settings
}

#[tauri::command]
pub fn set_theme(app: tauri::AppHandle, theme: String) -> Option<String> {
    let file = state_file(&app)?;
    edit(&file, |settings| settings.theme = Some(theme))
        .theme()
        .map(str::to_string)
}

#[tauri::command]
pub fn set_language(app: tauri::AppHandle, language: String) -> Option<String> {
    let file = state_file(&app)?;
    edit(&file, |settings| settings.language = Some(language))
        .language()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("shimbox-settings-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_written_choice_is_the_one_read_back() {
        let root = scratch("round-trip");
        let file = root.join(FILE);

        assert_eq!(read(&file).theme(), None, "no file is no choice");
        assert_eq!(read(&file).language(), None);

        write(
            &file,
            &Settings {
                theme: Some("dark".to_string()),
                language: Some("ru".to_string()),
            },
        );
        assert_eq!(read(&file).theme(), Some("dark"));
        assert_eq!(read(&file).language(), Some("ru"));

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

        fs::write(&file, r#"{"language":"de"}"#).unwrap();
        assert_eq!(read(&file).language(), None);

        fs::write(&file, "not json at all").unwrap();
        assert_eq!(read(&file).theme(), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_key_the_app_does_not_know_leaves_the_choices_alone() {
        let root = scratch("extra-key");
        let file = root.join(FILE);

        // Written by a newer build: what it carries is still what to use.
        fs::write(&file, r#"{"theme":"light","language":"ru","font":"comic"}"#).unwrap();
        assert_eq!(read(&file).theme(), Some("light"));
        assert_eq!(read(&file).language(), Some("ru"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn one_choice_leaves_the_other_one_asked() {
        let root = scratch("both");
        let file = root.join(FILE);

        edit(&file, |settings| settings.language = Some("ru".to_string()));
        edit(&file, |settings| settings.theme = Some("dark".to_string()));

        let settings = read(&file);
        assert_eq!(settings.theme(), Some("dark"));
        assert_eq!(settings.language(), Some("ru"), "a theme is not a reset");

        fs::remove_dir_all(&root).ok();
    }
}
