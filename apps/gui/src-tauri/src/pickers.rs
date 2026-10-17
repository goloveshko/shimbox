//! Where each Browse button last looked (docs/techspec.md 5). A dialog that
//! opens where the previous one left off makes every field repeat another
//! field's journey, so a directory is remembered per field and kept in
//! `$APPCONFIG/.pickers.json`, the way the window keeps its own geometry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::state;

const FILE: &str = ".pickers.json";

/// The directory to open `key`'s dialog at. What the field holds right now wins:
/// a path typed or filled in there says more about this moment than an older
/// pick does. Otherwise the field's own last directory, and only while it still
/// exists. Nothing at all leaves the choice to the system.
fn start_of(fields: &BTreeMap<String, String>, key: &str, typed: &str) -> Option<PathBuf> {
    existing_directory(Path::new(typed.trim())).or_else(|| {
        fields
            .get(key)
            .map(PathBuf::from)
            .filter(|stored| stored.is_dir())
    })
}

/// A directory as itself; anything else as the directory holding it, which
/// covers both a file and a path still being typed — a half-written name still
/// says which folder is meant. A path with no existing directory in it is
/// nothing to start from.
fn existing_directory(path: &Path) -> Option<PathBuf> {
    let candidate = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    candidate.is_dir().then_some(candidate)
}

// The rule in `state`, bound to this file's name and its shape: the directories
// the fields last ended at, where a file that cannot be read or understood is an
// empty one and losing it cannot be allowed to stop a dialog from opening.
fn read(file: &Path) -> BTreeMap<String, String> {
    state::read(file)
}

fn write(file: &Path, fields: &BTreeMap<String, String>) {
    state::write(file, fields);
}

fn state_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    state::file(app, FILE)
}

pub fn start_directory(app: &tauri::AppHandle, key: &str, typed: &str) -> Option<PathBuf> {
    let file = state_file(app)?;
    start_of(&read(&file), key, typed)
}

/// Record where `key` ended up, keeping every other field's directory.
pub fn remember(app: &tauri::AppHandle, key: &str, picked: &Path) {
    let Some(file) = state_file(app) else {
        return;
    };
    let mut fields = read(&file);
    let directory = existing_directory(picked).unwrap_or_else(|| picked.to_path_buf());
    fields.insert(key.to_string(), directory.display().to_string());
    write(&file, &fields);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        crate::testutil::scratch("pickers", name)
    }

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn a_field_holds_its_own_directory() {
        let root = scratch("per-field");
        let shims = root.join("shims");
        let tools = root.join("tools");
        fs::create_dir_all(&shims).unwrap();
        fs::create_dir_all(&tools).unwrap();

        let stored = fields(&[("destination", &shims.display().to_string())]);

        // The target field never sees the destination's folder.
        assert_eq!(
            start_of(&stored, "target", &tools.display().to_string()),
            Some(tools.clone())
        );
        assert_eq!(
            start_of(&stored, "target", ""),
            None,
            "a field with no history of its own starts where the system likes"
        );
        assert_eq!(start_of(&stored, "destination", ""), Some(shims));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_typed_path_starts_where_it_points() {
        let root = scratch("typed");
        let tools = root.join("tools");
        fs::create_dir_all(&tools).unwrap();
        let target = tools.join("ffmpeg.exe");
        fs::write(&target, b"MZ").unwrap();

        let empty = BTreeMap::new();
        assert_eq!(
            start_of(&empty, "target", &target.display().to_string()),
            Some(tools.clone()),
            "a file starts in the folder holding it"
        );
        assert_eq!(
            start_of(
                &empty,
                "target",
                &tools.join("half-typed.e").display().to_string()
            ),
            Some(tools.clone()),
            "a path that does not exist yet starts in its parent"
        );
        assert_eq!(
            start_of(&empty, "target", "   "),
            None,
            "an empty field is not a directory"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_vanished_directory_is_not_a_starting_point() {
        let root = scratch("gone");
        let was = root.join("was-here");
        let stored = fields(&[("source", &was.display().to_string())]);

        assert_eq!(start_of(&stored, "source", ""), None);

        fs::create_dir_all(&was).unwrap();
        assert_eq!(start_of(&stored, "source", ""), Some(was));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_map_survives_the_file_and_garbage_does_not() {
        let root = scratch("round-trip");
        let file = root.join(FILE);

        write(
            &file,
            &fields(&[("destination", r"C:\Shims"), ("target", r"C:\Tools")]),
        );
        let read_back = read(&file);
        assert_eq!(read_back.len(), 2);
        assert_eq!(
            read_back.get("destination").map(String::as_str),
            Some(r"C:\Shims")
        );

        // A remembered field is added without dropping the others.
        let mut grown = read_back;
        grown.insert("source".to_string(), r"C:\Tools\bin".to_string());
        write(&file, &grown);
        assert_eq!(read(&file).len(), 3);

        fs::write(&file, "not json at all").unwrap();
        assert!(read(&file).is_empty());

        fs::remove_dir_all(&root).ok();
    }
}
