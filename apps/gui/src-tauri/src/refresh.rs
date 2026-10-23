//! Mode 3: the runners already on disk, brought up to the bytes this build
//! embeds (docs/techspec.md 5.7). Windows only.
//!
//! A shim is a copy of a runner, so a fix to a runner reaches none of the shims
//! made before it. No list of them is kept, because a pair says where it is: one
//! folder answers with every pair inside it, and `%PATH%` answers with every
//! folder that was ever meant to be reached by name - the same variable
//! [`crate::folder_on_path`] reads.
//!
//! Reaching a folder by name is also how the scan reaches another tool's files:
//! Scoop keeps its own shims in a `%PATH%` folder under a `.shim` of its own, so
//! the extension marks a candidate, never ours. Two things settle it, both read
//! out of the runner itself rather than inferred from where it sits: the two names
//! of its version resource, which say that the file came from this tool and which
//! of its binaries it was built as, and a byte-for-byte comparison with the runner
//! this build embeds (5.7).

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
    /// The row is chosen by its config and written through its runner, so both
    /// halves travel with it: one to ask for, the other to show.
    config: String,
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

/// The `.shim` files directly inside `root`, one level deep, as candidates for a
/// pair: a batch writes into one folder rather than a tree, `%PATH%` names folders
/// rather than trees, and walking into `C:\Windows` is a scan of a drive rather
/// than of a person's shims. Which of them are ours is [`ours`], never this.
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

/// A config read one line at a time, with the exact bytes that ended each one. A
/// file that passed through two editors carries two kinds of ending, and the
/// endings are part of what the manager does not own.
fn lines_of(text: &str) -> Vec<(&str, &str)> {
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(end) = rest.find('\n') {
        let crlf = end > 0 && rest.as_bytes()[end - 1] == b'\r';
        let cut = if crlf { end - 1 } else { end };
        lines.push((&rest[..cut], &rest[cut..end + 1]));
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        lines.push((rest, ""));
    }
    lines
}

/// The config with its stamp pointing at these bytes. Only the stamp line is
/// rewritten: every other line goes back with its own bytes and its own ending,
/// and a config with no stamp line gets one last, ended like the lines above it -
/// the person's file is being kept here, not rearranged.
fn restamped(text: &str, stamp: &str) -> String {
    let line = format!("runner={stamp}");
    let lines = lines_of(text);
    let stamp_at = lines.iter().position(|(content, _)| {
        content
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == "runner")
    });

    let mut written = String::with_capacity(text.len() + line.len() + 2);
    for (index, (content, terminator)) in lines.iter().enumerate() {
        if Some(index) == stamp_at {
            written.push_str(&line);
        } else {
            written.push_str(content);
        }
        written.push_str(terminator);
    }
    if stamp_at.is_none() {
        let newline = lines
            .iter()
            .rev()
            .find(|(_, terminator)| !terminator.is_empty())
            .map_or("\n", |(_, terminator)| *terminator);
        if !written.is_empty() && !written.ends_with(newline) {
            written.push_str(newline);
        }
        written.push_str(&line);
        written.push_str(newline);
    }
    written
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

/// The config's own word that it belongs to a shimbox shim, which is all a pair
/// with no runner left can offer: the header line [`shim::HEADER`] over what it
/// took as its first line, or a stamp of the one shape only this manager writes.
/// A `.shim` that says neither is some other tool's file that happens to share the
/// extension (3.6).
fn declared(text: &str) -> bool {
    let header = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.trim_start_matches('\u{feff}') == shim::HEADER);
    // `cli@` and `gui@` are what [`shim::stamp`] writes, so a line of any other
    // shape was not written here.
    let shaped = stamp_of(text).is_some_and(|stamp| {
        stamp.split_once('@').is_some_and(|(variant, version)| {
            !version.is_empty() && matches!(variant, "cli" | "gui")
        })
    });
    header || shaped
}

/// Whether these are our files to write bytes over. A runner that is there
/// answers for itself, because only the bytes of one of our two runner builds name
/// both `shimbox` and the runner they were compiled as (5.7) - and it answers
/// instead of its config, so another tool's `.shim` beside another tool's `.exe` is
/// never made into a pair by the two files agreeing with each other. The manager
/// carries the same product name as its runners, so the name alone is not enough:
/// a `.shim` of the manager's own stem would otherwise ask us to write a runner
/// over the window that is being asked.
fn ours(exe: &Path, image: &pe::Image, text: &str) -> bool {
    if exe.is_file() {
        image.is_runner()
    } else {
        declared(text)
    }
}

/// Whether the bytes already on disk are exactly the ones this build embeds. A
/// version number cannot answer that: the runners are rebuilt while the crate
/// version stays put, and a shim stamped with a number this build still prints
/// would be reported current while it ran older bytes (5.7).
fn current(exe: &Path, variant: Variant) -> bool {
    fs::read(exe).is_ok_and(|bytes| bytes == variant.bytes())
}

fn row(config: &Path) -> Option<Stale> {
    let Ok(text) = fs::read_to_string(config) else {
        return None;
    };
    let exe = shim::exe_of(config);
    let image = pe::inspect(&exe);
    if !ours(&exe, &image, &text) {
        return None;
    }
    let stamp = stamp_of(&text);
    let variant = wanted(image.subsystem, stamp.as_deref());
    if current(&exe, variant) {
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
        config: config.display().to_string(),
        exe: exe.display().to_string(),
        selected: true,
    })
}

/// Lists the pairs [`ours`] accepts whose runner half is not this build's bytes.
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
///
/// The names come from a scan of the same session, but a row the person chose is
/// not taken on the scan's word alone: the guard runs again here, over the bytes
/// as they are now, because a folder on `%PATH%` can be written by anything while
/// the window stays open.
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
    let image = pe::inspect(&exe);
    if !ours(&exe, &image, &text) {
        // The half of a pair that is here now is not one of ours: another tool's
        // file under a name this scan once read as a shim.
        summary
            .failed
            .push(shim::fail("foreign", exe.display().to_string()));
        return;
    }
    let variant = wanted(image.subsystem, stamp_of(&text).as_deref());

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

    /// One pair of files inside the test's own folder: a config in the shape this
    /// manager writes and a runner of the asked-for variant that is ours but not
    /// current - the embedded bytes with one more behind them, so the resource
    /// still says `shimbox` while the comparison says older.
    fn pair_of(dir: &Path, alias: &str, stamp: &str, variant: Variant) -> PathBuf {
        let config = dir.join(format!("{alias}.shim"));
        fs::write(
            &config,
            format!("{}\nrunner={stamp}\npath=C:\\t.exe\n", shim::HEADER),
        )
        .unwrap();
        let mut bytes = variant.bytes().to_vec();
        bytes.push(0);
        fs::write(shim::exe_of(&config), bytes).unwrap();
        config
    }

    fn pair(dir: &Path, alias: &str, stamp: &str) -> PathBuf {
        pair_of(dir, alias, stamp, Variant::Cli)
    }

    /// Every test here writes real PE bytes, and Windows opens a new `.exe` to be
    /// looked at before anyone is let remove it, so the folder goes once that look
    /// ends rather than the instant the write did. Bounded, because a folder that
    /// is really held should fail this helper rather than wait for it.
    fn clear(dir: &Path) {
        for _ in 0..20 {
            if fs::remove_dir_all(dir).is_ok() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("a runner was left in TEMP: {}", dir.display());
    }

    fn summary() -> Summary {
        Summary {
            refreshed: Vec::new(),
            failed: Vec::new(),
        }
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
    fn a_config_without_its_runner_is_taken_at_its_own_word() {
        assert!(declared("# shimbox config\npath=C:\\t.exe\n"));
        // The mark Notepad puts in front of a file, and the blank lines a person
        // leaves above the header, still say who wrote the config.
        assert!(declared("\u{feff}# shimbox config\npath=C:\\t.exe\n"));
        assert!(declared("\n\n# shimbox config\npath=C:\\t.exe\n"));
        assert!(declared("runner=cli@0.9.0\npath=C:\\t.exe\n"));

        // What another tool writes into its own folder that sits on `%PATH%`: the
        // extension matches and nothing else does, so it never becomes a pair.
        assert!(!declared(
            "path=C:\\scoop\\apps\\git\\current\\bin\\git.cmd\n"
        ));
        assert!(!declared("runner=git\npath=C:\\t.exe\n"));
        assert!(!declared("runner=cli@\npath=C:\\t.exe\n"));
        assert!(!declared("path=C:\\t.exe\n# shimbox config\n"));
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

        // Two endings in one file, which is what a folder that saw two editors
        // holds: the line the manager owns changes and the others keep the exact
        // bytes they came with, including the last one's missing ending.
        assert_eq!(
            restamped(
                "path=C:\\a.exe\r\nrunner=cli@0.9.0\npath=C:\\b.exe",
                "cli@1.0.0"
            ),
            "path=C:\\a.exe\r\nrunner=cli@1.0.0\npath=C:\\b.exe"
        );

        // A line added to such a file joins it the way the file ends its lines.
        assert_eq!(
            restamped("path=C:\\a.exe\r\narg=x", "cli@1.0.0"),
            "path=C:\\a.exe\r\narg=x\r\nrunner=cli@1.0.0\r\n"
        );
    }

    #[test]
    fn a_pair_is_a_row_until_its_runner_is_this_builds_bytes() {
        let dir = crate::testutil::scratch("refresh", "stale");

        let current = shim::stamp(Variant::Cli);
        let fresh = pair(&dir, "current", &current);
        fs::write(shim::exe_of(&fresh), Variant::Cli.bytes()).unwrap();
        assert!(row(&fresh).is_none(), "{current} is this build");

        // The hole a stamp cannot close: the runners were rebuilt since the version
        // number was written, so this copy carries a line this build still prints
        // over bytes it no longer embeds. Only the comparison sees it.
        let drifted = pair(&dir, "drifted", &current);
        let found = row(&drifted).expect("older bytes behind a current stamp");
        assert_eq!(found.was.as_deref(), Some(current.as_str()));
        assert_eq!(found.variant, "cli");

        // A half pair - the config 5.3 leaves behind - is as stale as a shim can be,
        // and says so with nothing in `was`.
        fs::write(dir.join("bare.shim"), "# shimbox config\npath=C:\\t.exe\n").unwrap();
        assert_eq!(
            row(&dir.join("bare.shim"))
                .expect("no runner to compare")
                .was,
            None
        );

        // A stray `.exe` with no config beside it is not a pair either way: the
        // scan asks for the config, and a name of our own shape is not ownership.
        fs::write(dir.join("stray.shim"), "path=C:\\t.exe\n").unwrap();
        assert!(row(&dir.join("stray.shim")).is_none());

        clear(&dir);
    }

    #[test]
    fn a_runners_own_word_outranks_its_configs() {
        let dir = crate::testutil::scratch("refresh", "foreign");

        // A real binary of another tool, put beside a config written in our exact
        // shape - the two files another tool's `shims` folder drops on `%PATH%`,
        // and the pair this scan must never offer to overwrite.
        let foreign = fs::read(r"C:\Windows\System32\cmd.exe").unwrap();
        fs::write(dir.join("git.exe"), &foreign).unwrap();
        let config = dir.join("git.shim");
        fs::write(
            &config,
            "# shimbox config\nrunner=cli@0.9.0\npath=C:\\git\\bin\\git.cmd\n",
        )
        .unwrap();

        assert!(row(&config).is_none(), "an extension is not an owner");

        // Again over the names the person chose, because the folder they were
        // scanned from is writable by anything while the window stays open.
        let mut summary = summary();
        refresh_one(&config, &mut summary);
        assert!(summary.refreshed.is_empty());
        assert_eq!(summary.failed[0].code, "foreign");
        assert_eq!(
            fs::read(dir.join("git.exe")).unwrap(),
            foreign,
            "nothing was written over it"
        );
        assert!(
            fs::read_to_string(&config).unwrap().contains("git.cmd"),
            "nor into its config"
        );

        clear(&dir);
    }

    #[test]
    fn the_runners_this_build_embeds_answer_for_themselves() {
        // The identity rule reads what `crates/runner/winres.rs` puts into a binary
        // and `build.rs` embeds here, so this is the test that notices when either
        // half stops agreeing with the other: a runner that lost its name would
        // make the scan answer with an empty list forever.
        let dir = crate::testutil::scratch("refresh", "embedded");
        for variant in [Variant::Cli, Variant::Gui] {
            let exe = dir.join(format!("{}.exe", variant.name()));
            fs::write(&exe, variant.bytes()).unwrap();

            let image = pe::inspect(&exe);
            assert!(image.is_runner(), "{} lost its name", variant.name());
            assert_eq!(image.version.text(), Some(env!("CARGO_PKG_VERSION")));
            assert!(current(&exe, variant));

            // The header is what picks the variant, so the two copies stay told
            // apart by the one field a shim has to get right (5.4).
            let wanted = match variant.name() {
                "cli" => pe::Subsystem::Console,
                _ => pe::Subsystem::Gui,
            };
            assert_eq!(image.subsystem, wanted);
        }
        clear(&dir);
    }

    #[test]
    fn the_manager_is_ours_but_never_a_runner() {
        // `bin\shimbox.exe` and this test binary are built by the same build
        // script, so they carry the same version resource: our product name and no
        // runner's original file. A `.shim` of the manager's own stem is the one
        // pair a `%PATH%` scan could name that would ask it to write a runner over
        // the window being asked - which is what the second half of the identity
        // rule is for.
        let manager = std::env::current_exe().unwrap();
        let image = pe::inspect(&manager);
        assert_eq!(image.product.as_deref(), Some("shimbox"));
        assert_eq!(image.original, None);
        assert!(!image.is_runner());

        let dir = crate::testutil::scratch("refresh", "manager");
        let config = dir.join("shimbox.shim");
        fs::write(
            &config,
            "# shimbox config\nrunner=cli@0.9.0\npath=C:\\t.exe\n",
        )
        .unwrap();
        let exe = shim::exe_of(&config);
        fs::copy(&manager, &exe).unwrap();

        assert!(row(&config).is_none(), "a manager is not a stale shim");

        let before = fs::read(&exe).unwrap();
        let mut summary = summary();
        refresh_one(&config, &mut summary);
        assert!(summary.refreshed.is_empty());
        assert_eq!(summary.failed[0].code, "foreign");
        assert_eq!(
            fs::read(&exe).unwrap(),
            before,
            "and nothing was written over it"
        );

        clear(&dir);
    }

    #[test]
    fn a_pair_keeps_the_runner_it_was_made_with() {
        let dir = crate::testutil::scratch("refresh", "variant");

        let gui = pair_of(&dir, "editor", "gui@0.9.0", Variant::Gui);
        assert_eq!(row(&gui).expect("an old gui runner").variant, "gui");

        // A line that claims a detached runner over a console copy loses: the
        // header on disk is what answers whether a console window appears.
        let mixed = pair(&dir, "mixed", "gui@0.9.0");
        assert_eq!(row(&mixed).expect("old console bytes").variant, "cli");

        // And when only the config is left, its line is all there is to go on: the
        // variant that reports its own errors would otherwise lose the window.
        fs::remove_file(shim::exe_of(&gui)).unwrap();
        assert_eq!(
            row(&gui).expect("a half pair keeps its line").variant,
            "gui"
        );

        clear(&dir);
    }

    #[test]
    fn refreshing_writes_the_runner_and_its_own_line_of_the_config() {
        let dir = crate::testutil::scratch("refresh", "write");
        let config = pair(&dir, "zip", "cli@0.9.0");

        let mut summary = summary();
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

        clear(&dir);
    }

    #[test]
    fn refreshing_a_half_pair_finishes_it() {
        // The other order of the same rule: a config that outlived its runner is
        // given one, and the line that says which bytes went in.
        let dir = crate::testutil::scratch("refresh", "half");
        let config = dir.join("solo.shim");
        fs::write(&config, "# shimbox config\nrunner=gui@0.9.0\n").unwrap();

        let mut summary = summary();
        refresh_one(&config, &mut summary);

        assert!(summary.failed.is_empty());
        assert_eq!(
            fs::read(shim::exe_of(&config)).unwrap(),
            Variant::Gui.bytes()
        );
        assert!(row(&config).is_none(), "a finished pair is not a row");

        clear(&dir);
    }

    #[test]
    fn a_pair_that_moved_on_is_left_alone() {
        let dir = crate::testutil::scratch("refresh", "gone");
        let gone = pair(&dir, "tool", "cli@0.9.0");
        fs::remove_file(&gone).unwrap();

        let mut summary = summary();
        refresh_one(&gone, &mut summary);

        assert!(summary.refreshed.is_empty(), "no runner is written");
        assert_eq!(summary.failed[0].code, "missing");
        assert!(row(&gone).is_none(), "and the scan will not offer it again");

        clear(&dir);
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
