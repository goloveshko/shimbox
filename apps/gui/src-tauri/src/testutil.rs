//! One helper for the tests of every module: a folder under the system's temp
//! directory that a test may write into and delete when it is done.

use std::fs;
use std::path::PathBuf;

/// The process id keeps two builds tested at once out of each other's folder, and
/// `module` keeps two tests that named their case the same way - there are two
/// `round-trip` among them - out of the one folder they would otherwise have
/// deleted from under each other while the other was still reading it.
pub(crate) fn scratch(module: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("shimbox-{module}-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
