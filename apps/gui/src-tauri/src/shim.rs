//! Writes a shim pair: `<alias>.shim` first, then `<alias>.exe`
//! (docs/techspec.md 5.1, 5.3, 5.4). Windows only.

use std::fs;
use std::path::{Path, PathBuf};

use crate::pe::{self, Subsystem};
use crate::runner::Variant;

/// Everything the form holds, as raw strings: the UI does no parsing.
#[derive(serde::Deserialize)]
pub struct Request {
    target: String,
    destination: String,
    alias: String,
    arguments: String,
    variant: String,
    workdir: String,
    /// No rows at all is a request without an environment, which is what Mode 2
    /// generates for.
    #[serde(default)]
    environment: Vec<EnvRow>,
}

/// One row of the environment block. `kind` says where the value comes from:
/// `value` writes it into the config, `cred` and `file` write a reference the
/// runner resolves at launch, which is what keeps a secret out of a folder on
/// `PATH` (docs/techspec.md 3.5).
#[derive(serde::Deserialize)]
pub struct EnvRow {
    name: String,
    kind: String,
    value: String,
}

/// The only namespace shimbox reads credentials from, so a name typed into the
/// form cannot point the runner at some other program's stored secret.
pub const CRED_PREFIX: &str = "shimbox/";

#[derive(Debug, serde::Serialize)]
pub struct Created {
    pub exe: String,
    pub config: String,
    pub variant: &'static str,
}

/// A rejection the UI can translate: a stable code plus the one path that
/// explains it.
#[derive(Debug, serde::Serialize)]
pub struct Failure {
    pub code: &'static str,
    pub detail: Option<String>,
}

impl From<Subsystem> for Variant {
    /// An unreadable header falls back to the variant that can report its own
    /// errors at all (docs/techspec.md 5.4).
    fn from(subsystem: Subsystem) -> Variant {
        match subsystem {
            Subsystem::Gui => Variant::Gui,
            _ => Variant::Cli,
        }
    }
}

pub(crate) fn fail(code: &'static str, detail: impl Into<String>) -> Failure {
    Failure {
        code,
        detail: Some(detail.into()),
    }
}

#[tauri::command]
pub fn generate(request: Request) -> Result<Created, Failure> {
    let target = checked_path(&request.target, true, "target")?;
    let destination = checked_path(&request.destination, false, "destination")?;
    let alias = checked_alias(&request.alias)?;

    let variant = match request.variant.as_str() {
        "cli" => Variant::Cli,
        "gui" => Variant::Gui,
        // `Auto detect` is the default, so anything else resolves from the
        // target's own header rather than failing the request.
        _ => Variant::from(pe::subsystem_of(&target)),
    };

    // The form prefills the working directory with the target's own folder,
    // which is also what the runner falls back to (3.4). Writing that back into
    // the config would pin a path meant to keep following the target, so a
    // field left at the resolved default writes nothing.
    let workdir = match request.workdir.trim() {
        "" => "",
        dir if target
            .parent()
            .is_some_and(|parent| same_path(Path::new(dir), parent)) =>
        {
            ""
        }
        dir => dir,
    };

    write_pair(
        &target,
        &destination,
        &alias,
        &split_arguments(&request.arguments),
        workdir,
        &env_lines(&request.environment)?,
        variant,
    )
}

/// Turns the form's rows into the pairs that go into the config, keyed by the
/// line they produce. A row with no name is an empty line of the form and
/// writes nothing.
fn env_lines(rows: &[EnvRow]) -> Result<Vec<(&'static str, String)>, Failure> {
    let mut lines = Vec::new();
    for row in rows {
        let name = row.name.trim();
        if name.is_empty() {
            continue;
        }
        let (key, written) = match row.kind.as_str() {
            // Taken verbatim, as the runner reads it: a trailing space is the
            // user's, not noise to clean up.
            "value" => ("env", format!("{name}={}", row.value)),
            "cred" => {
                let target = row.value.trim();
                if !target.starts_with(CRED_PREFIX) {
                    return Err(fail("env", format!("{name}: {target}")));
                }
                ("envref", format!("{name}=cred:{target}"))
            }
            "file" => {
                let path = checked_path(&row.value, true, "env")?;
                ("envref", format!("{name}=file:{}", path.display()))
            }
            other => return Err(fail("env", format!("{name}: {other}"))),
        };
        lines.push((key, written));
    }
    Ok(lines)
}

/// Windows path equality ignores case, and a folder typed with a trailing
/// separator is the same folder without one; `components` drops both.
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    let left = a.components();
    let right = b.components();
    left.clone().count() == right.clone().count()
        && left
            .zip(right)
            .all(|(a, b)| a.as_os_str().eq_ignore_ascii_case(b.as_os_str()))
}

/// The config half of a pair. Its presence is what marks an `.exe` as a shim
/// rather than a binary, which is how the scan recognises one (5.2).
pub(crate) fn config_of(exe: &Path) -> PathBuf {
    exe.with_extension("shim")
}

/// Writes one shim pair, refusing to touch an existing one. Mode 1 and the
/// batch mode both end here, so the collision rule exists once (5.3).
pub fn write_pair(
    target: &Path,
    destination: &Path,
    alias: &str,
    arguments: &[String],
    workdir: &str,
    environment: &[(&'static str, String)],
    variant: Variant,
) -> Result<Created, Failure> {
    let exe_path = destination.join(format!("{alias}.exe"));
    let config_path = config_of(&exe_path);
    for path in [&config_path, &exe_path] {
        if path.exists() {
            return Err(fail("collision", path.display().to_string()));
        }
    }

    let text = config_text(target, arguments, workdir, environment)?;
    // The config lands first: a half-generated shim should be a working config
    // without a runner, never a runner reading a stale config (5.3).
    write(&config_path, text.as_bytes())?;
    write(&exe_path, variant.bytes())?;

    Ok(Created {
        exe: exe_path.display().to_string(),
        config: config_path.display().to_string(),
        variant: variant.name(),
    })
}

/// Resolves a form field into an absolute path that exists as the requested
/// kind.
pub(crate) fn checked_path(
    text: &str,
    want_file: bool,
    code: &'static str,
) -> Result<PathBuf, Failure> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(fail(code, String::new()));
    }
    let path = std::path::absolute(trimmed).map_err(|error| fail(code, error.to_string()))?;
    let exists = if want_file {
        path.is_file()
    } else {
        path.is_dir()
    };
    if !exists {
        return Err(fail(code, path.display().to_string()));
    }
    Ok(path)
}

/// Returns the name without its `.exe`, which the caller appends itself.
fn checked_alias(text: &str) -> Result<String, Failure> {
    let mut alias = text.trim();
    // People paste file names with the extension; generating `git.exe.exe`
    // would only be a surprise later.
    let bytes = alias.as_bytes();
    if bytes.len() >= 4 && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".exe") {
        alias = &alias[..alias.len() - 4];
    }
    if alias.is_empty() {
        return Err(fail("alias", String::new()));
    }
    if alias.chars().any(|c| {
        c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) || alias.ends_with('.')
        || alias.ends_with(' ')
    {
        return Err(fail("alias", alias.to_string()));
    }
    // Reserved device names look legal but cannot be created.
    let stem = alias.split('.').next().unwrap_or(alias).to_ascii_uppercase();
    if is_reserved(&stem) {
        return Err(fail("alias", alias.to_string()));
    }
    Ok(alias.to_string())
}

fn is_reserved(stem: &str) -> bool {
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix)
            .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|digit| digit.is_ascii_digit()))
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    fs::write(path, bytes).map_err(|error| fail("write", format!("{}: {error}", path.display())))
}

/// Splits the arguments field on whitespace while keeping quoted groups, and
/// the quotes, intact: the config stores values verbatim and the runner passes
/// them through `raw_arg`, so the child's own parser sees `"out file.txt"`
/// exactly as typed (docs/techspec.md 3.3).
fn split_arguments(text: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    arguments.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    arguments
}

fn config_text(
    target: &Path,
    arguments: &[String],
    workdir: &str,
    environment: &[(&str, String)],
) -> Result<String, Failure> {
    let mut text = String::from("# shimbox config\n");
    line(&mut text, "path", &target.display().to_string())?;
    for argument in arguments {
        line(&mut text, "arg", argument)?;
    }
    if !workdir.is_empty() {
        line(&mut text, "workdir", workdir)?;
    }
    for (key, pair) in environment {
        line(&mut text, key, pair)?;
    }
    Ok(text)
}

/// One `key=value` line of the config. A value carrying a newline would land as
/// a second line the runner reads as its own key, and `path` is arbitrary code
/// from there, so no field gets to contain one.
fn line(text: &mut String, key: &str, value: &str) -> Result<(), Failure> {
    if value.contains(['\r', '\n']) {
        return Err(fail("newline", format!("{key}={value}")));
    }
    text.push_str(&format!("{key}={value}\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_whitespace_and_keeps_quotes() {
        assert_eq!(
            split_arguments("  -o \"out file.txt\"  -y "),
            vec!["-o", "\"out file.txt\"", "-y"]
        );
        assert!(split_arguments("   ").is_empty());
    }

    #[test]
    fn writes_only_the_keys_that_are_set() {
        let text = config_text(
            Path::new(r"C:\Tools\ffmpeg\bin\ffmpeg.exe"),
            &["-hide_banner".to_string()],
            "",
            &[("env", "NO_COLOR=1".to_string())],
        )
        .unwrap();
        assert_eq!(
            text,
            "# shimbox config\npath=C:\\Tools\\ffmpeg\\bin\\ffmpeg.exe\narg=-hide_banner\nenv=NO_COLOR=1\n"
        );

        let text = config_text(Path::new(r"C:\t.exe"), &[], r"C:\Work", &[]).unwrap();
        assert_eq!(text, "# shimbox config\npath=C:\\t.exe\nworkdir=C:\\Work\n");
    }

    #[test]
    fn writes_each_kind_of_environment_row() {
        let dir = scratch("env");
        let token = dir.join("token.txt");
        fs::write(&token, "x").unwrap();

        let rows = |kind: &str, value: &str| {
            vec![
                EnvRow {
                    name: " IGNORED ".to_string(),
                    kind: "value".to_string(),
                    value: "x".to_string(),
                },
                EnvRow {
                    name: "GH_TOKEN".to_string(),
                    kind: kind.to_string(),
                    value: value.to_string(),
                },
            ]
        };

        let lines = env_lines(&rows("value", "plain ")).unwrap();
        // The name is trimmed, a literal value is not.
        assert_eq!(
            lines,
            [
                ("env", "IGNORED=x".to_string()),
                ("env", "GH_TOKEN=plain ".to_string())
            ]
        );

        let lines = env_lines(&rows("cred", "shimbox/github")).unwrap();
        assert_eq!(
            lines[1],
            ("envref", "GH_TOKEN=cred:shimbox/github".to_string())
        );

        let lines = env_lines(&rows("file", token.display().to_string().as_str())).unwrap();
        assert_eq!(
            lines[1],
            ("envref", format!("GH_TOKEN=file:{}", token.display()))
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_reference_it_cannot_resolve_later() {
        let row = |kind: &str, value: &str| EnvRow {
            name: "GH_TOKEN".to_string(),
            kind: kind.to_string(),
            value: value.to_string(),
        };

        // Another program's credential is not ours to hand to a child.
        assert_eq!(
            env_lines(&[row("cred", "GitHub/personal")]).unwrap_err().code,
            "env"
        );
        // A file that is not there would only fail when the shim runs.
        assert_eq!(
            env_lines(&[row("file", r"C:\nowhere\token.txt")]).unwrap_err().code,
            "env"
        );
        assert_eq!(
            env_lines(&[row("registry", "HKCU\\x")]).unwrap_err().code,
            "env"
        );
    }

    #[test]
    fn refuses_to_write_a_value_that_would_become_a_second_line() {
        let dir = scratch("newline");
        let target = dir.join("tool.exe");
        fs::write(&target, b"MZ").unwrap();

        let failure = generate(Request {
            target: target.display().to_string(),
            destination: dir.display().to_string(),
            alias: "probe".to_string(),
            arguments: String::new(),
            variant: "cli".to_string(),
            workdir: "C:\\Work\npath=C:\\evil.exe".to_string(),
            environment: Vec::new(),
        })
        .unwrap_err();

        assert_eq!(failure.code, "newline");
        assert!(failure.detail.unwrap().contains("path=C:\\evil.exe"));
        // Nothing landed: the pair is written whole or not at all.
        assert!(!config_of(&dir.join("probe.exe")).exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn accepts_a_plain_alias_and_its_trailing_exe() {
        assert_eq!(checked_alias("ffmpeg").unwrap(), "ffmpeg");
        assert_eq!(checked_alias("ffmpeg.exe").unwrap(), "ffmpeg");
        assert_eq!(checked_alias("FFMPEG.EXE").unwrap(), "FFMPEG");
        assert_eq!(checked_alias("  git  ").unwrap(), "git");
        assert_eq!(checked_alias("7z").unwrap(), "7z");
    }

    #[test]
    fn rejects_illegal_and_reserved_aliases() {
        for alias in ["", ".exe", "a/b", "a\\b", "a:b", "a*b", "a.", "con", "COM1.exe"] {
            assert!(checked_alias(alias).is_err(), "accepted {alias:?}");
        }
    }

    #[test]
    fn generates_a_pair_and_refuses_to_overwrite_it() {
        let dir = scratch("pair");
        let target = dir.join("target.exe");
        fs::write(&target, b"MZ").unwrap();

        let request = |alias: &str| Request {
            target: target.display().to_string(),
            destination: dir.display().to_string(),
            alias: alias.to_string(),
            arguments: "-v".to_string(),
            variant: "cli".to_string(),
            workdir: String::new(),
            environment: Vec::new(),
        };

        let created = generate(request("tool")).unwrap();
        assert_eq!(created.variant, "cli");
        assert!(Path::new(&created.exe).is_file());
        assert!(fs::read_to_string(&created.config).unwrap().contains("arg=-v"));

        let error = generate(request("tool")).unwrap_err();
        assert_eq!(error.code, "collision");
        assert!(error.detail.unwrap().ends_with("tool.shim"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_workdir_only_when_it_differs_from_the_targets_folder() {
        let dir = scratch("workdir");
        let nested = dir.join("bin");
        fs::create_dir_all(&nested).unwrap();
        let target = nested.join("tool.exe");
        fs::write(&target, b"MZ").unwrap();
        let own = nested.display().to_string();

        let config = |workdir: &str, alias: &str| {
            let created = generate(Request {
                target: target.display().to_string(),
                destination: dir.display().to_string(),
                alias: alias.to_string(),
                arguments: String::new(),
                variant: "cli".to_string(),
                workdir: workdir.to_string(),
                environment: Vec::new(),
            })
            .unwrap();
            fs::read_to_string(created.config).unwrap()
        };

        // The value the form prefills, however the user happens to type it.
        for (index, workdir) in ["", &own, &format!("{own}\\"), &own.to_uppercase()]
            .into_iter()
            .enumerate()
        {
            assert!(
                !config(workdir, &format!("a{index}")).contains("workdir="),
                "wrote the default for {workdir:?}"
            );
        }
        // A folder of the user's own is written, so the runner keeps it.
        let text = config(&dir.display().to_string(), "b");
        assert!(text.contains(&format!("workdir={}\n", dir.display())));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_missing_target_or_destination() {
        let dir = scratch("checks");
        let request = |target: &str, destination: &str| Request {
            target: dir.join(target).display().to_string(),
            destination: dir.join(destination).display().to_string(),
            alias: "tool".to_string(),
            arguments: String::new(),
            variant: "auto".to_string(),
            workdir: String::new(),
            environment: Vec::new(),
        };

        fs::write(dir.join("target.exe"), b"MZ").unwrap();
        assert_eq!(generate(request("nope.exe", "")).unwrap_err().code, "target");
        assert_eq!(
            generate(request("target.exe", "nowhere")).unwrap_err().code,
            "destination"
        );

        assert!(generate(request("target.exe", "")).is_ok());

        fs::remove_dir_all(&dir).ok();
    }

    /// A per-test folder under `TEMP`, removed again when the test ends.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shimbox-test-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
