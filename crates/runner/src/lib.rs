//! What both shim variants share: read `<own stem>.shim` next to us and launch
//! the target it names with the caller's arguments. Windows only. See
//! docs/techspec.md 4.
//!
//! The two crates that build on this one are the variants themselves, and what
//! they decide apart from each other is the PE subsystem, where a failure of the
//! shim is reported, and whether the child may open a console. The last of the
//! three is the argument `run` takes; the second is why it returns its failures
//! instead of printing them, since a shim with no console has no stderr.

mod config;
mod env;

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use config::Config;

/// Config file not found next to the executable.
const EXIT_NO_CONFIG: u32 = 101;
/// Config unreadable, or `path` missing.
const EXIT_BAD_CONFIG: u32 = 102;
/// Child could not be started.
const EXIT_SPAWN_FAILED: u32 = 103;
/// An `envref` names nothing readable, so the child would run without a value
/// its target expects (4).
const EXIT_BAD_ENV: u32 = 104;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCommandLineW() -> *const u16;
}

/// Failure that belongs to the shim itself, carrying the exit code to use. The
/// variant that catches this is the one that knows where the message can go.
pub struct ShimError {
    pub code: u32,
    pub message: String,
}

/// How the child is started, which is the one difference between the variants
/// that reaches inside the launch itself.
pub enum Launch {
    /// The shim has a console and the child inherits it, as a tool run from a
    /// terminal expects.
    Inherited,
    /// The shim has no console of its own, so a console target must not be left
    /// to allocate one behind its window.
    NoConsole,
}

pub fn run(launch: Launch) -> Result<u32, ShimError> {
    let own_path = std::env::current_exe().map_err(|error| ShimError {
        code: EXIT_BAD_CONFIG,
        message: format!("shimbox: cannot locate myself: {error}"),
    })?;

    let stem = own_path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| ShimError {
            code: EXIT_BAD_CONFIG,
            message: format!(
                "shimbox: cannot derive shim name from {}",
                own_path.display()
            ),
        })?;
    let config_path = own_path.with_file_name(format!("{stem}.shim"));

    let text = std::fs::read_to_string(&config_path).map_err(|error| ShimError {
        code: EXIT_NO_CONFIG,
        message: format!("shimbox: cannot read {}: {error}", config_path.display()),
    })?;
    let config = config::parse(&text);

    if config.path.is_empty() {
        return Err(ShimError {
            code: EXIT_BAD_CONFIG,
            message: format!("shimbox: no `path` in {}", config_path.display()),
        });
    }

    let target = absolute(Path::new(&config.path));
    let mut command = Command::new(&target);

    let arguments = child_arguments(&config);
    if !arguments.is_empty() {
        command.raw_arg(OsString::from_wide(&arguments));
    }

    // Working directory of the target, not of the caller: portable tools load
    // DLLs and data relative to their own folder (techspec 3.4).
    let workdir = match &config.workdir {
        Some(dir) => absolute(Path::new(dir)),
        None => target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf(),
    };
    command.current_dir(workdir);

    // Resolved before the child starts: a reference that names nothing fails the
    // shim rather than handing the target a value it expects and never gets.
    let references = env::resolve(&config.envref).map_err(|failure| ShimError {
        code: EXIT_BAD_ENV,
        message: failure.0,
    })?;

    // Merged into the caller's environment by std, so a name that is already
    // there is overwritten and an empty value sets it to empty rather than
    // removing it. References come second, so one wins over a plain value of the
    // same name.
    command.envs(
        config
            .env
            .iter()
            .map(|(name, value)| (name, value))
            .chain(references.iter().map(|(name, value)| (name, value))),
    );

    // Only the detached variant asks for this: a shim run from a terminal has a
    // console to inherit, and suppressing it would take the child's output away.
    if matches!(launch, Launch::NoConsole) {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let status = command.status().map_err(|error| ShimError {
        code: EXIT_SPAWN_FAILED,
        message: format!("shimbox: cannot start {}: {error}", target.display()),
    })?;

    Ok(status.code().unwrap_or(EXIT_SPAWN_FAILED as i32) as u32)
}

/// Command line for the child: config arguments first, then the caller's
/// arguments byte-for-byte. Built as UTF-16 so nothing is re-quoted or
/// re-escaped on the way (techspec FR-1.3).
fn child_arguments(config: &Config) -> Vec<u16> {
    let mut wide = Vec::new();
    for argument in &config.args {
        wide.push(b' ' as u16);
        wide.extend(OsStr::new(argument).encode_wide());
    }
    wide.extend(caller_tail());
    wide
}

/// Everything after the first argument of our own command line. The first
/// argument is always our own executable path, so an escaped quote inside it is
/// not worth handling.
fn caller_tail() -> Vec<u16> {
    let raw = unsafe { GetCommandLineW() };
    if raw.is_null() {
        return Vec::new();
    }

    let mut length = 0usize;
    while unsafe { *raw.add(length) } != 0 {
        length += 1;
    }
    let line = unsafe { std::slice::from_raw_parts(raw, length) };

    let mut index = skip_spaces(line, 0);
    if index >= line.len() {
        return Vec::new();
    }

    if line[index] == QUOTE {
        index += 1;
        while index < line.len() && line[index] != QUOTE {
            index += 1;
        }
        if index < line.len() {
            index += 1;
        }
    } else {
        while index < line.len() && !is_separator(line[index]) {
            index += 1;
        }
    }

    line[index..].to_vec()
}

const QUOTE: u16 = b'"' as u16;

fn skip_spaces(line: &[u16], from: usize) -> usize {
    let mut index = from;
    while index < line.len() && is_separator(line[index]) {
        index += 1;
    }
    index
}

fn is_separator(unit: u16) -> bool {
    unit == b' ' as u16 || unit == b'\t' as u16
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
