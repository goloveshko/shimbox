//! Reads the PE `Subsystem` field of a target executable so the UI can offer
//! `Auto detect` (docs/techspec.md 5.4).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Subsystem {
    Gui,
    Console,
    /// Not a PE we could read. The caller falls back to the CLI variant, which
    /// is the only one that can report its own errors.
    Unknown,
}

const DOS_HEADER_SIZE: usize = 64;
const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
const NT_HEADERS_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"
const COFF_HEADER_SIZE: usize = 20;
/// Signature + COFF header + enough of the optional header to reach the
/// subsystem field, which sits at the same offset in PE32 and PE32+.
const NT_HEADERS_SIZE: usize = 4 + COFF_HEADER_SIZE + 72;
const OPTIONAL_HEADER_SUBSYSTEM_OFFSET: usize = 68;
const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;
const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;

pub fn subsystem_of(path: &Path) -> Subsystem {
    // Real binaries keep the NT headers past the DOS stub, often around 0x100,
    // so seek to the offset the DOS header names instead of reading a prefix.
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Subsystem::Unknown,
    };

    let mut dos = [0u8; DOS_HEADER_SIZE];
    if file.read_exact(&mut dos).is_err() {
        return Subsystem::Unknown;
    }
    if u16::from_le_bytes([dos[0], dos[1]]) != IMAGE_DOS_SIGNATURE {
        return Subsystem::Unknown;
    }

    let nt_headers =
        u32::from_le_bytes([dos[0x3C], dos[0x3D], dos[0x3E], dos[0x3F]]) as u64;
    if nt_headers < DOS_HEADER_SIZE as u64 {
        return Subsystem::Unknown;
    }

    let mut nt = [0u8; NT_HEADERS_SIZE];
    if file
        .seek(SeekFrom::Start(nt_headers))
        .and_then(|_| file.read_exact(&mut nt))
        .is_err()
    {
        return Subsystem::Unknown;
    }
    parse_nt_headers(&nt)
}

/// Same walk over a whole image held in memory; used by the header tests.
#[cfg(test)]
fn subsystem_of_bytes(bytes: &[u8]) -> Subsystem {
    if bytes.len() < DOS_HEADER_SIZE {
        return Subsystem::Unknown;
    }
    if u16::from_le_bytes([bytes[0], bytes[1]]) != IMAGE_DOS_SIGNATURE {
        return Subsystem::Unknown;
    }

    let nt_headers =
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    let end = nt_headers + NT_HEADERS_SIZE;
    if nt_headers < DOS_HEADER_SIZE || bytes.len() < end {
        return Subsystem::Unknown;
    }
    parse_nt_headers(&bytes[nt_headers..end])
}

fn parse_nt_headers(nt: &[u8]) -> Subsystem {
    if u32::from_le_bytes([nt[0], nt[1], nt[2], nt[3]]) != NT_HEADERS_SIGNATURE {
        return Subsystem::Unknown;
    }

    let optional = &nt[4 + COFF_HEADER_SIZE..];
    let magic = u16::from_le_bytes([optional[0], optional[1]]);
    if magic != 0x10b && magic != 0x20b {
        return Subsystem::Unknown;
    }

    let offset = OPTIONAL_HEADER_SUBSYSTEM_OFFSET;
    match u16::from_le_bytes([optional[offset], optional[offset + 1]]) {
        IMAGE_SUBSYSTEM_WINDOWS_GUI => Subsystem::Gui,
        IMAGE_SUBSYSTEM_WINDOWS_CUI => Subsystem::Console,
        _ => Subsystem::Unknown,
    }
}

/// Byte layout of a minimal PE image carrying the given subsystem; tests only.
#[cfg(test)]
fn craft_image(subsystem: u16, nt_headers: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; nt_headers + NT_HEADERS_SIZE];
    bytes[0..2].copy_from_slice(b"MZ");
    bytes[0x3C..0x40].copy_from_slice(&(nt_headers as u32).to_le_bytes());
    bytes[nt_headers..nt_headers + 4].copy_from_slice(b"PE\0\0");

    let optional = nt_headers + 4 + COFF_HEADER_SIZE;
    bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    let field = optional + OPTIONAL_HEADER_SUBSYSTEM_OFFSET;
    bytes[field..field + 2].copy_from_slice(&subsystem.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gui_and_console() {
        assert_eq!(
            subsystem_of_bytes(&craft_image(IMAGE_SUBSYSTEM_WINDOWS_GUI, 0x80)),
            Subsystem::Gui
        );
        assert_eq!(
            subsystem_of_bytes(&craft_image(IMAGE_SUBSYSTEM_WINDOWS_CUI, 0x1F8)),
            Subsystem::Console
        );
    }

    #[test]
    fn rejects_non_pe_and_truncated_headers() {
        assert_eq!(subsystem_of_bytes(b"plain text"), Subsystem::Unknown);

        let mut truncated = craft_image(IMAGE_SUBSYSTEM_WINDOWS_GUI, 0x80);
        truncated.truncate(0x90);
        assert_eq!(subsystem_of_bytes(&truncated), Subsystem::Unknown);

        let mut dangling = craft_image(IMAGE_SUBSYSTEM_WINDOWS_GUI, 0x80);
        dangling[0x3C..0x40].copy_from_slice(&0xFFF0u32.to_le_bytes());
        assert_eq!(subsystem_of_bytes(&dangling), Subsystem::Unknown);
    }

    #[test]
    fn rejects_missing_signatures_and_unknown_subsystem() {
        let mut without_dos = craft_image(IMAGE_SUBSYSTEM_WINDOWS_GUI, 0x80);
        without_dos[0..2].copy_from_slice(b"XX");
        assert_eq!(subsystem_of_bytes(&without_dos), Subsystem::Unknown);

        let mut without_nt = craft_image(IMAGE_SUBSYSTEM_WINDOWS_GUI, 0x80);
        without_nt[0x80..0x84].copy_from_slice(b"EVIL");
        assert_eq!(subsystem_of_bytes(&without_nt), Subsystem::Unknown);

        let odd_subsystem = craft_image(0xFFFF, 0x80);
        assert_eq!(subsystem_of_bytes(&odd_subsystem), Subsystem::Unknown);
    }

    #[test]
    fn file_reader_reaches_the_nt_headers_of_real_binaries() {
        // The crafted cases above are small enough to be read as a prefix; only
        // real binaries exercise the seek.
        assert_eq!(
            subsystem_of(Path::new(r"C:\Windows\System32\cmd.exe")),
            Subsystem::Console
        );
        assert_eq!(
            subsystem_of(Path::new(r"C:\Windows\System32\notepad.exe")),
            Subsystem::Gui
        );
    }
}
