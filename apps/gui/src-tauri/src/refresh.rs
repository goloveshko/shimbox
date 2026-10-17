//! Mode 3: the runners already on disk, brought up to the bytes this build
//! embeds (docs/techspec.md 5.7). Windows only.
//!
//! A shim is a copy of a runner, so a fix to a runner reaches none of the shims
//! made before it. No list of them is kept, because a pair says where it is: one
//! folder answers with every pair inside it, and `%PATH%` answers with every
//! folder that was ever meant to be reached by name - the same variable
//! [`crate::folder_on_path`] reads.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pe;
use crate::runner::Variant;
use crate::shim::{self, Failure};

/// One pair whose runner half is not this build's bytes. `was` is the stamp its
/// config carries, and `None` is a pair made before stamps were written - which
/// is exactly as stale as a shim can be.
#[derive(serde::Serialize)]
pub struct Stale {
    name: String,
    exe: String,
    was: Option<String>,
    variant: &'static str,
    selected: bool,
}

#[derive(serde::Deserialize)]
pub struct Request {
    /// The config half of each pair to refresh: it is what marks a pair as one,
    /// and a stray `.exe` is not something to write runner bytes over.
    configs: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct Summary {
    refreshed: Vec<String>,
    failed: Vec<Failure>,
}

/// Where a scan looks: the folder the field names, or every folder on `%PATH%`
/// while it names nothing.
fn roots(folder: &str) -> Result<Vec<PathBuf>, Failure> {
    if folder.trim().is_empty() {
        return Ok(folders_of(
            std::env::var_os("Path").unwrap_or_default().as_os_str(),
        ));
    }
    Ok(vec![shim::checked_path(folder, false, "folder")?])
}

/// The entries of a `PATH`-shaped list that are folders, in order and without
/// the repeats the variable allows. The same reading rule as
/// [`crate::contains_folder`], answered with a list rather than a yes or no.
fn folders_of(path_list: &OsStr) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(path_list) {
        // An empty entry is the current directory to Windows and no answer about
        // a folder here; a folder the variable still names but that is gone
        // holds no pair either.
        if entry.as_os_str().is_empty() || !entry.is_dir() {
            continue;
        }
        if !found.iter().any(|seen| shim::same_path(seen, &entry)) {
            found.push(entry);
        }
    }
    found
}

/// The pairs directly inside `root`, one level deep: a batch writes into one
/// folder rather than a tree, `%PATH%` names folders rather than trees, and
/// walking into `C:\Windows` is a scan of a drive rather than of a person's shims.
fn pairs_in(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("shim"))
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|path| {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    found
}

/// The `runner` line of a config that is already in hand. The format is one
/// `key=value` line at a time (3.2), and this is the only key the manager reads
/// back: the key alone decides, so a value carrying an `=` keeps all of it and a
/// comment is no key at all.
fn stamp_of(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "runner")
        .map(|(_, value)| value.trim().to_string())
}

/// The config with its stamp pointing at these bytes. Every line that is not the
/// stamp goes back exactly as it came, and a config with no stamp line gets one
/// last: the person's file is being kept here, not rearranged.
fn restamped(text: &str, stamp: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("runner={stamp}");
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    match lines.iter().position(|entry| {
        entry
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == "runner")
    }) {
        Some(at) => lines[at] = line,
        None => lines.push(line),
    }
    lines.join(newline) + newline
}

/// Which of the two runners a pair wants. The header of the copy on disk decides,
/// because that is what answers whether a console window appears; the config's own
/// line decides only while there is no header to read - the half of a pair that is
/// still only a config, which 5.3 leaves on purpose (5.4).
fn wanted(subsystem: pe::Subsystem, stamp: Option<&str>) -> Variant {
    match subsystem {
        pe::Subsystem::Gui => Variant::Gui,
        pe::Subsystem::Console => Variant::Cli,
        pe::Subsystem::Unknown => match stamp {
            Some(text) if text.starts_with("gui") => Variant::Gui,
            _ => Variant::Cli,
        },
    }
}

fn row(config: &Path) -> Option<Stale> {
    let Ok(text) = fs::read_to_string(config) else {
        return None;
    };
    let exe = shim::exe_of(config);
    let stamp = stamp_of(&text);
    let variant = wanted(pe::subsystem_of(&exe), stamp.as_deref());
    if stamp.as_deref() == Some(shim::stamp(variant).as_str()) {
        return None;
    }
    Some(Stale {
        was: stamp,
        variant: variant.name(),
        name: exe
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        exe: exe.display().to_string(),
        selected: true,
    })
}

/// Lists the pairs whose runner half is not this build's.
#[tauri::command]
pub async fn scan_shims(folder: String) -> Result<Vec<Stale>, Failure> {
    Ok(roots(&folder)?
        .iter()
        .flat_map(|root| pairs_in(root).into_iter().filter_map(|config| row(&config)))
        .collect())
}

/// Rewrites the runner half of every pair named, and the one line of its config
/// that says which bytes are in there. What the person wrote is otherwise theirs:
/// a target, its arguments, an environment and a working directory all come back
/// unchanged. Each pair answers for itself, which is what lets one shim that is
/// running now fail on its own line rather than take the rest of the list down
/// with it.
#[tauri::command]
pub async fn refresh_runners(request: Request) -> Summary {
    let mut summary = Summary {
        refreshed: Vec::new(),
        failed: Vec::new(),
    };
    for config in &request.configs {
        refresh_one(Path::new(config), &mut summary);
    }
    summary
}

fn refresh_one(config: &Path, summary: &mut Summary) {
    let Ok(text) = fs::read_to_string(config) else {
        // The row was true when the scan said it. Since then the pair is gone, and
        // nothing here may invent a shim out of a leftover name.
        summary.failed.push(shim::fail(
            "missing",
            format!("{} is no longer there", config.display()),
        ));
        return;
    };
    let exe = shim::exe_of(config);
    let variant = wanted(pe::subsystem_of(&exe), stamp_of(&text).as_deref());

    // The bytes land first, which is the other way round from 5.3 and for the
    // other reason: a corrected line over an old runner would report a pair as
    // current when it is not, and that is the one answer worse than a stale row.
    if let Err(error) = fs::write(&exe, variant.bytes()) {
        summary
            .failed
            .push(shim::fail("write", format!("{}: {error}", exe.display())));
        return;
    }
    match fs::write(config, restamped(&text, &shim::stamp(variant))) {
        Ok(()) => summary.refreshed.push(exe.display().to_string()),
        Err(error) => summary.failed.push(shim::fail(
            "write",
            format!("{}: {error}", config.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pair of files inside the test's own folder. `not a PE` on purpose: a
    /// copy whose header cannot be read is decided by its config's own line, and
    /// that is the half of the rule a real binary would never exercise.
    fn pair(dir: &Path, alias: &str, stamp: &str) -> PathBuf {
        let config = dir.join(format!("{alias}.shim"));
        fs::write(
            &config,
            format!("# shimbox config\nrunner={stamp}\npath=C:\\t.exe\n"),
        )
        .unwrap();
        fs::write(shim::exe_of(&config), b"not a PE").unwrap();
        config
    }

    #[test]
    fn reads_the_runner_line_out_of_a_config() {
        let text = "runner=cli@1.0.0\npath=C:\\t.exe\n";
        assert_eq!(stamp_of(text).as_deref(), Some("cli@1.0.0"));

        // A config from before the line existed names no version, which is not the
        // same answer as a line written blank.
        assert_eq!(stamp_of("path=C:\\t.exe\n"), None);
        assert_eq!(stamp_of("runner=\npath=C:\\t.exe\n").as_deref(), Some(""));

        // A value is the rest of the line, and a comment is no line at all.
        assert_eq!(
            stamp_of("runner=cli@1.0.0+build=7\n").as_deref(),
            Some("cli@1.0.0+build=7")
        );
        assert_eq!(stamp_of("# runner=cli@1.0.0\n"), None);
    }

    #[test]
    fn restamping_keeps_every_line_that_is_not_the_stamp() {
        let written = restamped(
            "# shimbox config\r\npath=C:\\My Tools\\a.exe \r\narg=--out my file.txt\r\n\
             env=URL=https://x/?a=b\r\nrunner=cli@0.9.0\r\n",
            "cli@1.0.0",
        );

        assert_eq!(
            written,
            "# shimbox config\r\npath=C:\\My Tools\\a.exe \r\narg=--out my file.txt\r\n\
             env=URL=https://x/?a=b\r\nrunner=cli@1.0.0\r\n",
            "one line moves, and Notepad's own endings stay"
        );

        // A pair made before the line existed gets one, and gets nothing else.
        assert_eq!(
            restamped("path=C:\\t.exe\n", "cli@1.0.0"),
            "path=C:\\t.exe\nrunner=cli@1.0.0\n"
        );
    }

    #[test]
    fn a_pair_is_a_row_until_its_stamp_is_this_builds() {
        let dir = crate::testutil::scratch("refresh", "stale");

        let current = shim::stamp(Variant::Cli);
        assert!(
            row(&pair(&dir, "current", &current)).is_none(),
            "{current} is this build"
        );

        let older = pair(&dir, "older", "cli@0.9.0");
        let found = row(&older).expect("an older runner is a row");
        assert_eq!(found.was.as_deref(), Some("cli@0.9.0"));
        assert_eq!(found.name, "older");
        assert_eq!(found.variant, "cli");

        // A line this build cannot have written is not this build's stamp, and
        // neither is a pair with no line on it.
        assert!(row(&pair(&dir, "part", "cli")).is_some());
        fs::write(dir.join("bare.shim"), "path=C:\\t.exe\n").unwrap();
        fs::write(dir.join("bare.exe"), b"not a PE").unwrap();
        assert!(row(&dir.join("bare.shim")).is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pair_keeps_the_runner_it_was_made_with() {
        let dir = crate::testutil::scratch("refresh", "variant");
        let gui = pair(&dir, "editor", "gui@0.9.0");
        assert_eq!(row(&gui).expect("an old gui runner").variant, "gui");

        // And when only the config is left, its line is all there is to go on: the
        // variant that reports its own errors would otherwise lose the window.
        fs::remove_file(shim::exe_of(&gui)).unwrap();
        assert_eq!(
            wanted(pe::Subsystem::Unknown, Some("gui@0.9.0")).name(),
            "gui"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refreshing_writes_the_runner_and_its_own_line_of_the_config() {
        let dir = crate::testutil::scratch("refresh", "write");
        let config = pair(&dir, "zip", "cli@0.9.0");

        let mut summary = Summary {
            refreshed: Vec::new(),
            failed: Vec::new(),
        };
        refresh_one(&config, &mut summary);

        let exe = shim::exe_of(&config);
        assert!(summary.failed.is_empty());
        assert_eq!(summary.refreshed, [exe.display().to_string()]);
        assert_eq!(fs::read(&exe).unwrap(), Variant::Cli.bytes());

        let text = fs::read_to_string(&config).unwrap();
        assert_eq!(
            stamp_of(&text).as_deref(),
            Some(shim::stamp(Variant::Cli).as_str())
        );
        assert!(
            text.contains("path=C:\\t.exe"),
            "the target is not ours to rewrite"
        );
        assert!(text.contains("# shimbox config"), "nor is the comment");
        assert!(row(&config).is_none(), "a refreshed pair is not a row");

        // A new `.exe` is opened by Windows to be looked at before anyone is let
        // remove it, so the folder goes once the look ends rather than the instant
        // the write did. Bounded, because a folder that is really held should fail
        // this test rather than wait for it.
        let mut removed = false;
        for _ in 0..20 {
            removed = fs::remove_dir_all(&dir).is_ok();
            if removed {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(removed, "a refreshed runner was left in TEMP");
    }

    #[test]
    fn a_pair_that_moved_on_is_left_alone() {
        let dir = crate::testutil::scratch("refresh", "gone");
        let gone = pair(&dir, "tool", "cli@0.9.0");
        fs::remove_file(&gone).unwrap();

        let mut summary = Summary {
            refreshed: Vec::new(),
            failed: Vec::new(),
        };
        refresh_one(&gone, &mut summary);

        assert!(summary.refreshed.is_empty(), "no runner is written");
        assert_eq!(summary.failed[0].code, "missing");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn path_entries_are_read_the_way_windows_reads_them() {
        let existing = std::env::temp_dir();
        let variable = format!(
            "{};;{};{}",
            existing.display(),
            existing.display(),
            existing.join("no-such-folder").display()
        );

        assert_eq!(
            folders_of(std::ffi::OsString::from(variable).as_os_str()),
            vec![existing],
            "one folder twice is one folder"
        );
        assert!(roots("C:\\no-such-folder").is_err(), "a folder that is not");
    }
}
