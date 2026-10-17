//! Whether a newer release of shimbox is out (docs/techspec.md 5.6).
//!
//! The asking is the frontend's own - the answer is a public file and the window
//! already is a browser, so the one thing this module keeps is *when the
//! question was last asked*. That moment lives in `$APPCONFIG/.update.json`
//! beside the theme and the pickers' folders, and it is written whether or not
//! the ask answered: a machine with no network is not served by asking again on
//! every start.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

const FILE: &str = ".update.json";

/// Long enough that a day of restarts asks once, short enough that a release
/// does not sit unnoticed for a week.
const DAY: u64 = 60 * 60 * 24;

/// The moment of the last ask. A file that cannot be read is a never-asked,
/// which is what the window should do about it.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Asked {
    at: u64,
}

/// The release the frontend read. `version` is the tag as GitHub carries it, so
/// a leading `v` is expected here rather than stripped on the way in.
#[derive(serde::Deserialize)]
pub struct Release {
    version: String,
    url: String,
    notes: String,
}

/// What a card is made of: the same three, once the running version has been
/// told it is behind.
#[derive(serde::Serialize)]
pub struct Available {
    version: String,
    url: String,
    notes: String,
}

/// The numbers a version is made of. A leading `v` is how a tag is named, and
/// what follows a `-` or a `+` is a label rather than a number, so `v1.2.3-rc.1`
/// reads as 1.2.3 - which is never above 1.2.3 in the comparison below, and
/// that is the wanted answer for a pre-release.
fn numbers(version: &str) -> Vec<u64> {
    let core = version.trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or_default();
    core.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Only a bigger number is a newer release. Equal, smaller and unreadable all
/// answer no, so a tag that is not a version never brings a card.
fn is_newer(candidate: &str, running: &str) -> bool {
    let (candidate, running) = (numbers(candidate), numbers(running));
    for index in 0..candidate.len().max(running.len()) {
        let (a, b) = (
            candidate.get(index).copied().unwrap_or(0),
            running.get(index).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    false
}

fn due_at(asked: &Asked, now: u64) -> bool {
    now.saturating_sub(asked.at) >= DAY
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn read(file: &Path) -> Asked {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(file: &Path, asked: Asked) {
    let Ok(text) = serde_json::to_string_pretty(&asked) else {
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

/// Whether the frontend should ask GitHub now.
#[tauri::command]
pub fn update_due(app: tauri::AppHandle) -> bool {
    state_file(&app).is_some_and(|file| due_at(&read(&file), now()))
}

/// Records the ask, whatever it found, and answers whether it is worth a card.
/// The address comes off the network, so it goes to `open_link`, which only ever
/// hands a browser an address on the two hosts that card already names.
#[tauri::command]
pub fn update_seen(app: tauri::AppHandle, release: Option<Release>) -> Option<Available> {
    let file = state_file(&app)?;
    write(&file, Asked { at: now() });
    let release = release?;
    is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(Available {
        version: release.version,
        url: release.url,
        notes: release.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("shimbox-update-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_day_of_restarts_asks_once() {
        let root = scratch("gate");
        let file = root.join(FILE);

        assert!(due_at(&read(&file), 10 * DAY), "never asked is due");

        write(&file, Asked { at: 10 * DAY });
        assert!(!due_at(&read(&file), 10 * DAY + 60), "asked a minute ago");
        assert!(
            !due_at(&read(&file), 11 * DAY - 1),
            "asked just under a day ago"
        );
        assert!(due_at(&read(&file), 11 * DAY), "a day is a day");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_that_is_not_an_answer_reads_as_no_answer() {
        let root = scratch("garbage");
        let file = root.join(FILE);

        fs::write(&file, "not json").unwrap();
        assert_eq!(read(&file).at, 0);

        fs::write(&file, r#"{"at":"yesterday"}"#).unwrap();
        assert_eq!(read(&file).at, 0);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn only_a_bigger_number_is_a_newer_release() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1.0", "1.0.0"), "a tag's v is not part of it");
        assert!(is_newer("1.10.0", "1.9.0"), "1.10 is not 1.1");
        assert!(!is_newer("1.0", "1.0.0"), "a shorter name says the same");
        assert!(is_newer("2.0.0", "1.99.99"));

        assert!(!is_newer("1.0.0", "1.0.0"), "this build is not an update");
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"), "a pre-release is not it");
        assert!(!is_newer("", "1.0.0"), "no answer is no update");
        assert!(!is_newer("nightly", "1.0.0"), "a name is not a number");
    }
}
