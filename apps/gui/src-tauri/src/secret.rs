//! The credential store behind `envref=NAME=cred:shimbox/<name>`.
//!
//! A generic credential is the system's own named, per-user, encrypted slot:
//! shimbox writes one per secret, every shim points at it by name, and the
//! value never enters a config file in a folder on `PATH`
//! (docs/techspec.md 3.5).

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use crate::shim::{CRED_PREFIX, Failure, fail};

/// `CREDENTIALW` from `wincred.h`, field for field. The order is the system's
/// rather than ours, and a mirror that swaps two pointer fields hands
/// `CredWriteW` a null target name.
#[repr(C)]
struct CredentialW {
    flags: u32,
    kind: u32,
    target_name: *mut u16,
    comment: *mut u16,
    last_written: u64,
    blob_size: u32,
    blob: *mut u8,
    persist: u32,
    attribute_count: u32,
    attributes: *mut core::ffi::c_void,
    target_alias: *mut u16,
    /// Shown as the credential's user name; a generic one has none, so this is
    /// where the name the user typed goes.
    user_name: *mut u16,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
    fn CredDeleteW(target_name: *const u16, kind: u32, flags: u32) -> i32;
    fn CredEnumerateW(
        filter: *const u16,
        flags: u32,
        count: *mut u32,
        credentials: *mut *mut *mut CredentialW,
    ) -> i32;
    fn CredFree(credentials: *mut CredentialW);
}

const CRED_TYPE_GENERIC: u32 = 1;
/// Survives signing out, which is the point of storing a token here rather than
/// in a process environment, and does not roam to another machine. The header
/// numbers these from 1: `0` is `CRED_PERSIST_NONE`, which `CredWrite` rejects.
const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;
/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`, checked here because the system's own error
/// for exceeding it says nothing of the sort.
const MAX_BLOB: usize = 5 * 512;

fn wide(text: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
    wide.push(0);
    wide
}

/// A name the user can still read in the system's own list, and no control
/// characters that would end up in a config line.
///
/// The characters a file name refuses are refused here too, as far as they reach:
/// a credential is not a file, so a reserved device name stays a usable name, but
/// the config line is read verbatim (docs/techspec.md 3.4) and the target put in
/// it is looked up exactly, so a name carrying `\` or `"` is one that only the
/// store can answer for and no person can type back in. The reference's own `:`
/// stays allowed: the config cuts the line at the first `=` and the runner cuts
/// `cred:` at its first colon, so everything after it reaches the store whole,
/// and a name is not a file name trying to be one.
fn checked_name(name: &str) -> Result<&str, Failure> {
    let name = name.trim();
    let usable = !name.is_empty()
        && name
            .chars()
            .all(|c| c >= ' ' && !matches!(c, '<' | '>' | '"' | '/' | '\\' | '|' | '?' | '*'));
    if usable {
        Ok(name)
    } else {
        Err(fail("secret", name.to_string()))
    }
}

/// Stores one secret and returns the name a config refers to.
#[tauri::command]
pub fn store_secret(name: String, value: String) -> Result<String, Failure> {
    let name = checked_name(&name)?;
    if value.is_empty() || value.chars().any(|c| c < ' ') {
        return Err(fail("secret", name.to_string()));
    }
    if value.len() > MAX_BLOB {
        return Err(fail(
            "length",
            format!("{name}: {} > {MAX_BLOB}", value.len()),
        ));
    }

    let target = format!("{CRED_PREFIX}{name}");
    let target_wide = wide(&target);
    let user_name_wide = wide(name);
    let blob = value.as_bytes();

    let record = CredentialW {
        flags: 0,
        kind: CRED_TYPE_GENERIC,
        target_name: target_wide.as_ptr() as *mut u16,
        comment: std::ptr::null_mut(),
        last_written: 0,
        blob_size: blob.len() as u32,
        blob: blob.as_ptr() as *mut u8,
        persist: CRED_PERSIST_LOCAL_MACHINE,
        attribute_count: 0,
        attributes: std::ptr::null_mut(),
        target_alias: std::ptr::null_mut(),
        // The system's list shows this instead of the target, so it is the name
        // the user typed rather than the path it lives under.
        user_name: user_name_wide.as_ptr() as *mut u16,
    };

    if unsafe { CredWriteW(&record, 0) } == 0 {
        return Err(fail(
            "write",
            format!("{target}: {}", std::io::Error::last_os_error()),
        ));
    }
    Ok(target)
}

/// Removes one, refusing to touch anything outside the shimbox namespace. The
/// system's own panel can of course still delete it.
#[tauri::command]
pub fn forget_secret(target: String) -> Result<(), Failure> {
    if !target.starts_with(CRED_PREFIX) {
        return Err(fail("secret", target));
    }
    let target_wide = wide(&target);
    if unsafe { CredDeleteW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        return Err(fail(
            "write",
            format!("{target}: {}", std::io::Error::last_os_error()),
        ));
    }
    Ok(())
}

/// Every secret shimbox holds, by the name a config refers to, oldest last so
/// the picker reads the same each time.
#[tauri::command]
pub fn list_secrets() -> Vec<String> {
    let filter = wide(&format!("{CRED_PREFIX}*"));
    let mut count = 0u32;
    let mut block: *mut *mut CredentialW = std::ptr::null_mut();

    if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut block) } == 0 || block.is_null()
    {
        // Holding nothing is not an error worth reporting to the form.
        return Vec::new();
    }

    let mut names: Vec<String> = unsafe {
        (0..count as usize)
            .filter_map(|index| {
                let record = *block.add(index);
                if record.is_null() {
                    return None;
                }
                let name = (*record).target_name;
                if name.is_null() {
                    return None;
                }
                let mut length = 0;
                while *name.add(length) != 0 {
                    length += 1;
                }
                Some(
                    OsString::from_wide(std::slice::from_raw_parts(name, length))
                        .to_string_lossy()
                        .into_owned(),
                )
            })
            .collect()
    };
    unsafe { CredFree(block as *mut CredentialW) };

    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round trip through the real store, which is the only way to know the
    /// struct layout and the flags are right. The name is this process's own and
    /// nothing a person would type: a fixed one is a credential that already exists
    /// by the time the test runs, and the test would write over it and then delete
    /// the answer. The removal at the end is only of what the store just made.
    #[test]
    fn stores_lists_and_removes_one_credential() {
        let name = format!("_probe_{}", std::process::id());
        let target = store_secret(name.clone(), "token-test-value".to_string()).unwrap();
        assert_eq!(target, format!("{CRED_PREFIX}{name}"));

        assert!(list_secrets().contains(&target), "{target} not listed");

        forget_secret(target.clone()).unwrap();
        assert!(!list_secrets().contains(&target), "{target} left behind");
    }

    #[test]
    fn refuses_names_and_targets_that_are_not_ours() {
        assert_eq!(
            store_secret("a/b".to_string(), "x".to_string())
                .unwrap_err()
                .code,
            "secret"
        );
        assert_eq!(
            store_secret("good".to_string(), String::new())
                .unwrap_err()
                .code,
            "secret"
        );
        assert_eq!(
            store_secret("good".to_string(), "line\nbreak".to_string())
                .unwrap_err()
                .code,
            "secret"
        );
        assert_eq!(
            forget_secret("GitHub/personal".to_string())
                .unwrap_err()
                .code,
            "secret"
        );
    }

    #[test]
    fn refuses_a_name_no_one_could_type_back_into_a_reference() {
        for name in ["a\\b", "a/b", "a|b", "a<b", "a>b", "a*b", "a?b", "a\"b"] {
            assert_eq!(
                store_secret(name.to_string(), "x".to_string())
                    .unwrap_err()
                    .code,
                "secret",
                "accepted {name:?}"
            );
        }
        // A device name is a file rule, and a credential is not a file. The name
        // is this process's own and the store is left holding nothing.
        let name = format!("_probe_con_{}", std::process::id());
        let target = store_secret(name, "x".to_string()).unwrap();
        forget_secret(target).unwrap();

        // The reference keeps its own colon: `cred:` is cut at the first one and
        // everything after it is the target, so this is one whole credential.
        let colon = store_secret("a:b".to_string(), "x".to_string()).unwrap();
        assert_eq!(colon, format!("{CRED_PREFIX}a:b"));
        forget_secret(colon).unwrap();
    }
}
