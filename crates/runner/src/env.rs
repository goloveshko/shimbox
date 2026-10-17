//! Resolving `envref` into a value: a reference says where the value lives
//! instead of holding it, so the config on `PATH` stays free of secrets
//! (docs/techspec.md 3.5).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// A named reference could not be resolved, or names an unknown scheme.
#[derive(Debug)]
pub struct Unresolved(pub String);

/// Every reference in order; a later one wins over an earlier one of the same
/// name, which is how `Command::envs` treats the whole list.
pub fn resolve(references: &[(String, String)]) -> Result<Vec<(String, String)>, Unresolved> {
    let mut values = Vec::with_capacity(references.len());
    for (name, target) in references {
        values.push((name.clone(), read(name, target)?));
    }
    Ok(values)
}

/// `scheme:rest` — `file` reads a path, `cred` reads a generic credential.
fn read(name: &str, target: &str) -> Result<String, Unresolved> {
    let (scheme, rest) = target.split_once(':').ok_or_else(|| syntax(name, target))?;
    match scheme.to_ascii_lowercase().as_str() {
        "file" => file(name, rest),
        "cred" => credential(name, target, rest),
        _ => Err(syntax(name, target)),
    }
}

fn syntax(name: &str, target: &str) -> Unresolved {
    Unresolved(format!(
        "shimbox: `envref={name}` points at `{target}`, which is not a `file:` path or a `cred:` credential"
    ))
}

fn file(name: &str, path: &str) -> Result<String, Unresolved> {
    let mut text = std::fs::read_to_string(path).map_err(|error| {
        Unresolved(format!(
            "shimbox: cannot read the file `envref={name}` points at ({path}): {error}"
        ))
    })?;
    // A value file usually ends with a newline the author never meant as part
    // of the value. Spaces are left alone: they may be meant.
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    Ok(text)
}

/// The blob of a generic credential holds the value as UTF-8, which is what the
/// manager writes. Tools like `cmdkey` store UTF-16 instead, and a UTF-8 value
/// can hold no NUL, so a blob that does is read as UTF-16.
fn blob_to_text(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        let wide: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        return String::from_utf16(&wide).ok();
    }
    std::str::from_utf8(bytes).map(String::from).ok()
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn CredReadW(
        target_name: *const u16,
        kind: u32,
        flags: u32,
        credential: *mut *mut CredentialW,
    ) -> i32;
    fn CredFree(credential: *mut CredentialW);
}

/// `CREDENTIALW` from `wincred.h`, field for field. Duplicated from the GUI on
/// purpose: the runner is a binary crate with no library to share it from, and
/// the layout belongs to the system, not to us.
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
    user_name: *mut u16,
}

const CRED_TYPE_GENERIC: u32 = 1;

fn credential(name: &str, target: &str, stored_as: &str) -> Result<String, Unresolved> {
    let mut wide: Vec<u16> = OsStr::new(stored_as).encode_wide().collect();
    wide.push(0);

    let mut record: *mut CredentialW = std::ptr::null_mut();
    let found = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut record) };
    if found == 0 || record.is_null() {
        return Err(Unresolved(format!(
            "shimbox: `envref={name}` points at the credential `{target}`, which is not there"
        )));
    }

    let bytes = unsafe {
        let record = &*record;
        // A credential that holds no bytes holds no pointer with them, and a slice
        // is only allowed to name bytes that exist, so the empty value is built
        // rather than read out of a null that would stand for nothing.
        if record.blob.is_null() || record.blob_size == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(record.blob, record.blob_size as usize).to_vec()
        }
    };
    unsafe { CredFree(record) };

    blob_to_text(&bytes).ok_or_else(|| {
        Unresolved(format!(
            "shimbox: the credential `{target}` that `envref={name}` points at does not hold readable text"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file a test writes is gone when that test ends, including when it ends by
    /// failing: one left in `%TEMP%` reads as an answer to whoever finds it, and one
    /// fixed name is two runs of the suite answering to each other.
    struct Probe(std::path::PathBuf);

    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn reads_a_value_from_a_file_without_its_newlines() {
        let probe =
            Probe(std::env::temp_dir().join(format!("shimbox-envref-{}.txt", std::process::id())));
        std::fs::write(&probe.0, "ghp_from_file\r\n").unwrap();
        let target = format!("file:{}", probe.0.display());
        let resolved = resolve(&[("GH_TOKEN".to_string(), target)]).unwrap();
        assert_eq!(resolved[0].1, "ghp_from_file");
    }

    #[test]
    fn reports_a_missing_file_instead_of_passing_nothing() {
        let error = resolve(&[(
            "GH_TOKEN".to_string(),
            r"file:C:\nowhere\token.txt".to_string(),
        )])
        .unwrap_err();
        assert!(error.0.contains("envref=GH_TOKEN"), "{}", error.0);
        assert!(error.0.contains("C:\\nowhere"), "{}", error.0);
    }

    #[test]
    fn rejects_a_target_that_is_not_a_reference() {
        for target in ["C:\\Tools\\token.txt", "cred:", "https://x"] {
            let error = resolve(&[("X".to_string(), target.to_string())]).unwrap_err();
            assert!(error.0.contains(target), "{}", error.0);
        }
    }

    #[test]
    fn reads_a_blob_as_utf8_or_utf16() {
        assert_eq!(blob_to_text(b"ghp_plain").as_deref(), Some("ghp_plain"));
        let wide: Vec<u16> = "ghp_wide".encode_utf16().collect();
        let bytes: Vec<u8> = wide.iter().flat_map(|unit| unit.to_ne_bytes()).collect();
        assert_eq!(blob_to_text(&bytes).as_deref(), Some("ghp_wide"));
        assert_eq!(blob_to_text(&[0xff, 0xfe, 0xff]), None);
    }
}
