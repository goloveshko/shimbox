//! Shim runner: reads `<own stem>.shim` next to itself and launches the target
//! with the caller's arguments. Windows only. See docs/techspec.md 4.

// The subsystem is baked in at build time; the runner never branches on it.
#![cfg_attr(feature = "gui", windows_subsystem = "windows")]

mod config;

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};

use config::Config;

/// Config file not found next to the executable.
const EXIT_NO_CONFIG: u32 = 101;
/// Config unreadable, or `path` missing.
const EXIT_BAD_CONFIG: u32 = 102;
/// Child could not be started.
const EXIT_SPAWN_FAILED: u32 = 103;

#[cfg(feature = "gui")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCommandLineW() -> *const u16;
}

#[cfg(feature = "gui")]
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

/// Failure that belongs to the shim itself, carrying the exit code to use.
struct ShimError {
    code: u32,
    message: String,
}

fn main() {
    match run() {
        Ok(code) => exit(code as i32),
        Err(error) => fail(error.code, &error.message),
    }
}

fn run() -> Result<u32, ShimError> {
    let own_path = std::env::current_exe().map_err(|error| ShimError {
        code: EXIT_BAD_CONFIG,
        message: format!("shimbox: cannot locate myself: {error}"),
    })?;

    let stem = own_path.file_stem().and_then(OsStr::to_str).ok_or_else(|| ShimError {
        code: EXIT_BAD_CONFIG,
        message: format!("shimbox: cannot derive shim name from {}", own_path.display()),
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

    // Merged into the caller's environment by std, so a name that is already
    // there is overwritten and an empty value blanks it rather than removing it.
    command.envs(config.env.iter().map(|(name, value)| (name, value)));

    #[cfg(feature = "gui")]
    command.creation_flags(CREATE_NO_WINDOW);

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

#[cfg(not(feature = "gui"))]
fn fail(code: u32, message: &str) -> ! {
    eprintln!("{message}");
    exit(code as i32)
}

#[cfg(feature = "gui")]
fn fail(code: u32, message: &str) -> ! {
    // This variant has no console, so stderr would reach nobody.
    let text = wide_with_nul(message);
    let caption = wide_with_nul("shimbox");
    const MB_ICONERROR: u32 = 0x10;
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR,
        )
    };
    exit(code as i32)
}

#[cfg(feature = "gui")]
fn wide_with_nul(text: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
    wide.push(0);
    wide
}
