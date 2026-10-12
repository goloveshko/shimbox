//! The console-subsystem shim: it keeps the console it was started in, so the
//! target inherits stdin, stdout and stderr and the wait comes from the OS, and
//! a failure of the shim itself is a line on stderr. See docs/techspec.md 2.1.

use std::process::exit;

fn main() {
    match runner::run(runner::Launch::Inherited) {
        Ok(code) => exit(code as i32),
        Err(error) => {
            eprintln!("{}", error.message);
            exit(error.code as i32)
        }
    }
}
