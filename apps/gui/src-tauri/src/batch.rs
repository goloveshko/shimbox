//! Mode 2: one folder in, a shim for every binary inside it
//! (docs/techspec.md 5.2). Windows only.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tauri::Emitter;

use crate::pe;
use crate::runner::Variant;
use crate::shim::{self, Failure};

/// One discovered binary, named by the stem the shim will get, with the
/// variant `Auto` resolves to and the version that decides which of two
/// same-named files the table preselects (5.2).
#[derive(serde::Serialize)]
pub struct Found {
    name: String,
    path: String,
    size: u64,
    version: Option<String>,
    variant: &'static str,
    selected: bool,
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

/// A scanned file with everything the row and the choice need, kept apart from
/// `Found` so the comparison can be tested without a disk.
struct Candidate {
    name: String,
    path: PathBuf,
    size: u64,
    numbers: Option<[u32; 4]>,
    version: Option<String>,
    modified: SystemTime,
    selected: bool,
}

impl Candidate {
    fn of(path: PathBuf) -> Self {
        let metadata = fs::metadata(&path).ok();
        let size = metadata.as_ref().map_or(0, fs::Metadata::len);
        let modified = metadata
            .and_then(|meta| meta.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let version = pe::version_of(&path);
        let text = version.text().map(str::to_owned);
        Self {
            numbers: version.numbers(),
            version: text.or_else(|| {
                version
                    .numbers()
                    .map(|[a, b, c, d]| format!("{a}.{b}.{c}.{d}"))
            }),
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            size,
            modified,
            path,
            selected: false,
        }
    }
}

#[tauri::command]
pub async fn scan(
    source: String,
    destination: String,
    recursive: bool,
) -> Result<Vec<Found>, Failure> {
    let root = shim::checked_path(&source, false, "source")?;
    // The destination is only an exclusion hint here: half-typing it must not
    // break a scan of a folder that is perfectly readable.
    let exclude = std::path::absolute(destination.trim()).ok();
    let mut rows = walk(&root, recursive, exclude.as_deref())
        .into_iter()
        .map(Candidate::of)
        .collect::<Vec<_>>();
    mark_newest(&mut rows);
    Ok(rows.into_iter().map(Found::from).collect())
}

/// For each run of files that would claim the same shim name, select the one
/// the table starts with. `walk` sorts by name, so a group is always
/// contiguous, and a group of one is selected by definition.
fn mark_newest(rows: &mut [Candidate]) {
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        let folded = rows[start].name.to_lowercase();
        while end < rows.len() && rows[end].name.to_lowercase() == folded {
            end += 1;
        }
        for row in &mut rows[start..end] {
            row.selected = false;
        }
        let group = &rows[start..end];
        let best = group
            .iter()
            .enumerate()
            .max_by_key(|(_, row)| freshness(row))
            .map_or(start, |(index, _)| start + index);
        rows[best].selected = true;
        start = end;
    }
}

/// One key for "the freshest we can prove", because the rule cannot be a
/// pairwise comparison: with `version if both have one, else date`, an
/// unversioned new file beats a versioned one, and that one beats a second
/// versioned file the unversioned one loses to, so the winner would depend on
/// the order the rows happen to be scanned in. A file that reports a version
/// is a known quantity, so it goes first; date settles everything the version
/// cannot, which covers the tools that ship with no version resource at all.
fn freshness(row: &Candidate) -> (bool, [u32; 4], SystemTime) {
    (
        row.numbers.is_some(),
        row.numbers.unwrap_or([0; 4]),
        row.modified,
    )
}

impl From<Candidate> for Found {
    fn from(row: Candidate) -> Self {
        Self {
            version: row.version,
            name: row.name,
            path: row.path.display().to_string(),
            size: row.size,
            variant: Variant::from(pe::subsystem_of(&row.path)).name(),
            selected: row.selected,
        }
    }
}

#[tauri::command]
pub async fn generate_batch(app: tauri::AppHandle, request: Request) -> Result<Summary, Failure> {
    let destination = shim::checked_path(&request.destination, false, "destination")?;
    let total = request.targets.len();
    Ok(run_batch(&request.targets, &destination, |done| {
        let _ = app.emit("batch-progress", Progress { done, total });
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

/// Lists the `.exe` files below `root`, leaving out two kinds of file that are
/// never worth a row: one that is already a shim (its sibling `.shim` is the
/// other half of the pair), and everything inside `exclude`, which is where the
/// pairs are being written. Subdirectories that cannot be read are passed over:
/// a scan of a whole drive has to survive the ones it is not allowed to open.
fn walk(root: &Path, recursive: bool, exclude: Option<&Path>) -> Vec<PathBuf> {
    if exclude.is_some_and(|skip| shim::same_path(root, skip)) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                // A junction can point back up the tree, and there is no
                // visited set to break that cycle with.
                let inside = exclude.is_some_and(|skip| shim::same_path(&path, skip));
                if recursive && !file_type.is_symlink() && !inside {
                    pending.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                && !shim::config_of(&path).exists()
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
fn run_batch(targets: &[String], destination: &Path, mut progress: impl FnMut(usize)) -> Summary {
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

fn generate_one(
    target: &str,
    destination: &Path,
    claimed: &mut HashSet<String>,
    summary: &mut Summary,
) {
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
    match shim::write_pair(path, destination, alias, &[], "", &[], variant) {
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
    use std::time::Duration;

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

        assert_eq!(names(&walk(&root, false, None)), ["root.exe"]);
        assert_eq!(names(&walk(&root, true, None)), ["Deep.EXE", "root.exe"]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn walk_leaves_out_existing_shims_and_the_destination() {
        let root = scratch("filtered");
        let inside = root.join("bin");
        let other = root.join("other");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(root.join("real.exe"), b"MZ").unwrap();
        // A pair from an earlier run: the exe is only a runner copy.
        fs::write(root.join("shimmed.exe"), b"MZ").unwrap();
        fs::write(root.join("shimmed.shim"), b"path=C:\\t.exe\n").unwrap();
        fs::write(inside.join("written.exe"), b"MZ").unwrap();
        fs::write(inside.join("written.shim"), b"path=C:\\t.exe\n").unwrap();
        fs::write(other.join("later.exe"), b"MZ").unwrap();

        assert_eq!(names(&walk(&root, true, None)), ["later.exe", "real.exe"]);
        // The output folder is skipped as a whole, pairs and all.
        assert_eq!(
            names(&walk(&root, true, Some(&inside))),
            ["later.exe", "real.exe"]
        );
        // Case is not part of a Windows path identity.
        assert_eq!(
            names(&walk(&root, true, Some(&root.join("BIN")))),
            ["later.exe", "real.exe"]
        );
        assert!(walk(&root, true, Some(&root)).is_empty());

        fs::remove_dir_all(&root).ok();
    }

    fn candidate(name: &str, numbers: Option<[u32; 4]>, age_secs: u64) -> Candidate {
        Candidate {
            name: name.to_string(),
            path: PathBuf::from(name),
            size: 0,
            numbers,
            version: None,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 - age_secs),
            selected: false,
        }
    }

    #[test]
    fn selects_the_best_row_of_each_name_group() {
        let mut rows = vec![
            // A reported version outranks a recent date, and a higher version
            // outranks a lower one.
            candidate("ffmpeg", Some([5, 1, 0, 0]), 30),
            candidate("ffmpeg", Some([6, 0, 0, 0]), 90),
            candidate("ffmpeg", None, 0),
            // Same name, different case, no versions: the newer file on disk.
            candidate("FFPROBE", None, 40),
            candidate("ffprobe", None, 10),
            candidate("zip", None, 0),
        ];

        mark_newest(&mut rows);

        let chosen: Vec<(&str, Option<[u32; 4]>)> = rows
            .iter()
            .filter(|row| row.selected)
            .map(|row| (row.name.as_str(), row.numbers))
            .collect();
        assert_eq!(
            chosen,
            [
                ("ffmpeg", Some([6, 0, 0, 0])),
                ("ffprobe", None),
                ("zip", None)
            ]
        );
    }

    #[test]
    fn compares_versions_before_dates() {
        let versioned = candidate("a", Some([2, 0, 0, 0]), 200);
        let unversioned_but_recent = candidate("a", None, 0);
        assert!(freshness(&versioned) > freshness(&unversioned_but_recent));

        // Same version, and no version at all: the date decides.
        let older = candidate("a", Some([2, 0, 0, 0]), 300);
        assert!(freshness(&versioned) > freshness(&older));
        let recent = candidate("a", None, 0);
        let ancient = candidate("a", None, 500);
        assert!(freshness(&recent) > freshness(&ancient));
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
