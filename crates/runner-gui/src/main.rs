//! The windows-subsystem shim: it has no console of its own, so the target is
//! started without one and a failure of the shim is a message box, there being
//! no stderr for it to reach. See docs/techspec.md 2.1 and FR-1.6.

// The subsystem is a property of the bytes rather than a branch at run time:
// this crate is the detached variant and `runner-cli` is the console one.
#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::process::exit;

#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

fn main() {
    match runner::run(runner::Launch::NoConsole) {
        Ok(code) => exit(code as i32),
        Err(error) => {
            report(&error.message);
            exit(error.code as i32)
        }
    }
}

fn report(message: &str) {
    const MB_ICONERROR: u32 = 0x10;
    let text = wide_with_nul(message);
    let caption = wide_with_nul("shimbox");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR,
        )
    };
}

fn wide_with_nul(text: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
    wide.push(0);
    wide
}
