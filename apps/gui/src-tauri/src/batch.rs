//! Mode 2: one folder in, a shim for every binary inside it
//! (docs/techspec.md 5.2). Windows only.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::Emitter;

use crate::pe;
use crate::runner::Variant;
use crate::shim::{self, Failure};

/// One discovered binary, named by the stem the shim will get, with the
/// variant `Auto` resolves to, so the table previews the result (5.4).
#[derive(serde::Serialize)]
pub struct Found {
    name: String,
    path: String,
    variant: &'static str,
}

#[derive(serde::Deserialize)]
pub struct Request {
    targets: Vec<String>,
    destination: String,
}

#[derive(serde::Serialize)]
pub struct Skipped {
    path: String,
    reason: &'static str,
}

#[derive(serde::Serialize)]
pub struct Summary {
    created: Vec<String>,
    skipped: Vec<Skipped>,
}

#[tauri::command]
pub async fn scan(source: String, recursive: bool) -> Result<Vec<Found>, Failure> {
    let root = shim::checked_path(&source, false, "source")?;
    Ok(walk(&root, recursive)
        .into_iter()
        .map(|path| Found {
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.display().to_string(),
            variant: Variant::from(pe::subsystem_of(&path)).name(),
        })
        .collect())
}

#[tauri::command]
pub async fn generate_batch(app: tauri::AppHandle, request: Request) -> Result<Summary, Failure> {
    let destination = shim::checked_path(&request.destination, false, "destination")?;
    let total = request.targets.len();
    Ok(run_batch(&request.targets, &destination, |done| {
        let _ = app.emit(
            "batch-progress",
            Progress {
                done,
                total,
            },
        );
    }))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

/// Lists the `.exe` files below `root`. Subdirectories that cannot be read are
/// passed over: a scan of a whole drive has to survive the ones it is not
/// allowed to open.
fn walk(root: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                // A junction can point back up the tree, and there is no
                // visited set to break that cycle with.
                if recursive && !file_type.is_symlink() {
                    pending.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                found.push(path);
            }
        }
    }
    found.sort_by_key(|path| file_name(path).to_lowercase());
    found
}

/// Writes one shim per target and reports the rest. `progress` runs after
/// every target, including the ones that were skipped.
fn run_batch(
    targets: &[String],
    destination: &Path,
    mut progress: impl FnMut(usize),
) -> Summary {
    let mut summary = Summary {
        created: Vec::new(),
        skipped: Vec::new(),
    };
    // Two binaries in different subfolders can share a stem; the first one
    // claims the name and the rest are reported, never renamed (5.3).
    let mut claimed: HashSet<String> = HashSet::new();

    for (index, target) in targets.iter().enumerate() {
        generate_one(target, destination, &mut claimed, &mut summary);
        progress(index + 1);
    }

    summary
}

fn generate_one(target: &str, destination: &Path, claimed: &mut HashSet<String>, summary: &mut Summary) {
    let path = Path::new(target);
    let alias = match path.file_stem().and_then(OsStr::to_str) {
        Some(alias) if !alias.is_empty() && path.is_file() => alias,
        _ => {
            summary.skipped.push(skipped(target, "missing"));
            return;
        }
    };
    if !claimed.insert(alias.to_lowercase()) {
        summary.skipped.push(skipped(target, "duplicate"));
        return;
    }

    let variant = Variant::from(pe::subsystem_of(path));
    match shim::write_pair(path, destination, alias, &[], "", variant) {
        Ok(created) => summary.created.push(created.exe),
        Err(failure) => summary.skipped.push(skipped(target, failure.code)),
    }
}

fn skipped(path: &str, reason: &'static str) -> Skipped {
    Skipped {
        path: path.to_string(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shimbox-batch-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths.iter().map(|path| file_name(path)).collect()
    }

    #[test]
    fn walk_lists_executables_and_respects_the_recursion_flag() {
        let root = scratch("walk");
        let nested = root.join("tools").join("deep");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("root.exe"), b"MZ").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(nested.join("Deep.EXE"), b"MZ").unwrap();

        assert_eq!(names(&walk(&root, false)), ["root.exe"]);
        assert_eq!(names(&walk(&root, true)), ["Deep.EXE", "root.exe"]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn generates_every_row_and_reports_the_duplicate_stem() {
        let source = scratch("source");
        let destination = scratch("destination");
        let nested = source.join("sub");
        fs::create_dir_all(&nested).unwrap();
        let first = nested.join("git.exe");
        let second = source.join("git.exe");
        let other = source.join("zip.exe");
        let gone = source.join("gone.exe");
        for path in [&first, &second, &other] {
            fs::write(path, b"MZ").unwrap();
        }
        let targets: Vec<String> = vec![&first, &second, &other, &gone]
            .into_iter()
            .map(|path| path.display().to_string())
            .collect();

        let mut counts = Vec::new();
        let summary = run_batch(&targets, &destination, |done| counts.push(done));

        assert_eq!(summary.created.len(), 2);
        assert_eq!(destination.join("git.shim").is_file(), true);
        assert_eq!(destination.join("zip.exe").is_file(), true);
        let reasons: Vec<&str> = summary.skipped.iter().map(|row| row.reason).collect();
        assert_eq!(reasons, ["duplicate", "missing"]);
        // The count advances over the skipped rows too, or the bar stalls.
        assert_eq!(counts, [1, 2, 3, 4]);

        fs::remove_dir_all(&source).ok();
        fs::remove_dir_all(&destination).ok();
    }
}
