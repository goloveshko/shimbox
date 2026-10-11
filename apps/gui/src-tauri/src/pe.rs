//! Reads what a target executable says about itself without running it: the
//! `Subsystem` behind `Auto detect` (docs/techspec.md 5.4) and the file version
//! behind the batch table (5.2).

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

    let nt_headers = u32::from_le_bytes([dos[0x3C], dos[0x3D], dos[0x3E], dos[0x3F]]) as u64;
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

/// What a version resource says about a file. Both fields are optional: plenty
/// of shipped tools carry no version at all, and the batch table then shows the
/// row without one rather than guessing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Version {
    numbers: Option<[u32; 4]>,
    text: Option<String>,
}

impl Version {
    /// The scan itself has no use for this: it prints whatever it found, blank
    /// or not, so only the tests ask.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.numbers.is_none() && self.text.is_none()
    }

    /// The four numbers Windows compares, which is what decides the order of
    /// two candidates for one shim name.
    pub fn numbers(&self) -> Option<[u32; 4]> {
        self.numbers
    }

    /// The vendor's own string, because that is what the tool and Explorer show
    /// and `96.0` reads better than `96.0.0.0`.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

const RESOURCE_DIRECTORY_HEADER_SIZE: usize = 16;
const RESOURCE_DIRECTORY_ENTRY_SIZE: usize = 8;
const RESOURCE_DATA_ENTRY_SIZE: usize = 8;
const RESOURCE_NAMED_ENTRY: u32 = 0x8000_0000;
const RT_VERSION: u32 = 16;
const DATA_DIRECTORY_SIZE: usize = 8;
const IMAGE_DIRECTORY_ENTRY_RESOURCE: usize = 2;
const SECTION_HEADER_SIZE: usize = 40;
const FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF_04BD;
const FIXED_FILE_INFO_SIZE: usize = 52;
const MAX_SECTIONS: usize = 96;
const MAX_DIRECTORY_ENTRIES: usize = 512;
const MAX_VERSION_BLOCK: usize = 64 * 1024;

/// Reads the version resource of an executable. Returns an empty `Version` for
/// anything that is not a PE, has no resources, or has a resource tree that
/// does not parse — a scan must not fail over one odd binary.
pub fn version_of(path: &Path) -> Version {
    let Ok(mut file) = File::open(path) else {
        return Version::default();
    };
    let Some((resource_rva, sections)) = layout_of(&mut file) else {
        return Version::default();
    };
    let Some(data) = version_resource(&mut file, &sections, resource_rva) else {
        return Version::default();
    };
    parse_version_info(&data)
}

/// The resource directory's RVA plus the section table that maps it to a file
/// offset. Reads the headers once for both.
fn layout_of(file: &mut File) -> Option<(u32, Vec<Section>)> {
    let mut dos = [0u8; DOS_HEADER_SIZE];
    file.read_exact(&mut dos).ok()?;
    if u16::from_le_bytes([dos[0], dos[1]]) != IMAGE_DOS_SIGNATURE {
        return None;
    }

    let nt_headers = u32::from_le_bytes([dos[0x3C], dos[0x3D], dos[0x3E], dos[0x3F]]) as u64;
    if nt_headers < DOS_HEADER_SIZE as u64 {
        return None;
    }
    // Enough for the COFF header and the data directories of either optional
    // header shape; the section table is read separately right after it.
    let nt = read_at(file, nt_headers, 4 + COFF_HEADER_SIZE + 240)?;
    if u32::from_le_bytes([nt[0], nt[1], nt[2], nt[3]]) != NT_HEADERS_SIGNATURE {
        return None;
    }

    let section_count = u16_at(&nt, 4 + 2)? as usize;
    let optional_size = u16_at(&nt, 4 + 16)? as usize;
    let optional = 4 + COFF_HEADER_SIZE;
    let directories = match u16_at(&nt, optional)? {
        0x10b => optional + 96,
        0x20b => optional + 112,
        _ => return None,
    };
    let resource = directories + IMAGE_DIRECTORY_ENTRY_RESOURCE * DATA_DIRECTORY_SIZE;
    let resource_rva = u32_at(&nt, resource)?;
    if resource_rva == 0 {
        return None;
    }

    let sections_start = nt_headers + (4 + COFF_HEADER_SIZE + optional_size) as u64;
    let count = section_count.min(MAX_SECTIONS);
    let raw = read_at(file, sections_start, count * SECTION_HEADER_SIZE)?;
    let sections = raw
        .chunks_exact(SECTION_HEADER_SIZE)
        .filter_map(|header| Section::parse(header))
        .collect::<Vec<_>>();
    if sections.is_empty() {
        return None;
    }
    Some((resource_rva, sections))
}

#[derive(Debug, Clone, Copy)]
struct Section {
    virtual_address: u32,
    raw_size: u32,
    raw_pointer: u32,
}

impl Section {
    fn parse(header: &[u8]) -> Option<Self> {
        Some(Self {
            virtual_address: u32_at(header, 12)?,
            raw_size: u32_at(header, 16)?,
            raw_pointer: u32_at(header, 20)?,
        })
    }

    /// Bounded by the raw size, so an RVA that names bytes past what the
    /// section actually holds is rejected rather than seeked to.
    fn to_offset(&self, rva: u32) -> Option<u64> {
        let inside = rva.checked_sub(self.virtual_address)?;
        if inside >= self.raw_size {
            return None;
        }
        Some(u64::from(self.raw_pointer) + u64::from(inside))
    }
}

/// Walks `type -> name -> language` and returns the version block those three
/// levels point at. Every offset in a resource directory is relative to the
/// start of the resource section, which is `base`, and only the last level
/// holds data: the language entries are the leaf of the directory tree.
fn version_resource(file: &mut File, sections: &[Section], root_rva: u32) -> Option<Vec<u8>> {
    let base = sections
        .iter()
        .find_map(|section| section.to_offset(root_rva))?;

    let entries = directory(file, base)?;
    let version = follow(&entries, RT_VERSION)?;
    // The next two levels are a single named resource and a single language,
    // and both are conventionally the only one there is.
    let entries = directory(file, base + u64::from(version))?;
    let name = entries.first()?.1 & !RESOURCE_NAMED_ENTRY;
    let entries = directory(file, base + u64::from(name))?;
    let data = entries.first()?.1;
    if data & RESOURCE_NAMED_ENTRY != 0 {
        return None;
    }

    let entry = read_at(file, base + u64::from(data), RESOURCE_DATA_ENTRY_SIZE)?;
    let size = usize::try_from(u32_at(&entry, 4)?)
        .ok()?
        .min(MAX_VERSION_BLOCK);
    let offset = sections
        .iter()
        .find_map(|section| section.to_offset(u32_at(&entry, 0)?))?;
    read_at(file, offset, size)
}

/// The entries of one resource directory, as `(name or id, offset)`.
fn directory(file: &mut File, at: u64) -> Option<Vec<(u32, u32)>> {
    let header = read_at(file, at, RESOURCE_DIRECTORY_HEADER_SIZE)?;
    let named = usize::from(u16_at(&header, 12)?);
    let ids = usize::from(u16_at(&header, 14)?);
    let count = named.checked_add(ids)?.min(MAX_DIRECTORY_ENTRIES);

    let start = at + RESOURCE_DIRECTORY_HEADER_SIZE as u64;
    let raw = read_at(
        file,
        start,
        count.checked_mul(RESOURCE_DIRECTORY_ENTRY_SIZE)?,
    )?;
    Some(
        raw.chunks_exact(RESOURCE_DIRECTORY_ENTRY_SIZE)
            .filter_map(|entry| Some((u32_at(entry, 0)?, u32_at(entry, 4)?)))
            .collect(),
    )
}

fn follow(entries: &[(u32, u32)], id: u32) -> Option<u32> {
    entries
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, offset)| offset & !RESOURCE_NAMED_ENTRY)
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 || len > MAX_VERSION_BLOCK {
        return None;
    }
    let mut bytes = vec![0u8; len];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn align4(at: usize) -> usize {
    (at + 3) & !3
}

/// A UTF-16LE string up to its null, with the offset just past it.
fn utf16_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let mut units = Vec::new();
    let mut cursor = at;
    loop {
        let unit = u16_at(bytes, cursor)?;
        cursor += 2;
        if unit == 0 {
            return Some((String::from_utf16_lossy(&units), cursor));
        }
        units.push(unit);
    }
}

/// One node of a version-info tree: `length` bytes from `at`, a value of
/// `value_length` at `value_at`, and children padded to four bytes after it.
struct Block<'a> {
    bytes: &'a [u8],
    at: usize,
    length: usize,
    value_length: usize,
    value_at: usize,
    key: String,
}

impl<'a> Block<'a> {
    fn parse(bytes: &'a [u8], at: usize) -> Option<Self> {
        let length = usize::from(u16_at(bytes, at)?);
        if length < 6 || at + length > bytes.len() {
            return None;
        }
        let value_length = usize::from(u16_at(bytes, at + 2)?);
        let (key, after_key) = utf16_at(bytes, at + 6)?;
        Some(Self {
            bytes,
            at,
            length,
            value_length,
            value_at: align4(after_key),
            key,
        })
    }

    fn value(&self) -> &'a [u8] {
        let end = (self.value_at + self.value_length).min(self.bytes.len());
        self.bytes.get(self.value_at..end).unwrap_or(&[])
    }

    fn children(&self) -> Vec<Self> {
        let end = (self.at + self.length).min(self.bytes.len());
        let mut at = align4(self.value_at + self.value_length);
        let mut children = Vec::new();
        while at + 6 <= end {
            let Some(child) = Self::parse(self.bytes, at) else {
                break;
            };
            at = align4(at + child.length);
            children.push(child);
        }
        children
    }
}

fn parse_version_info(bytes: &[u8]) -> Version {
    let Some(root) = Block::parse(bytes, 0) else {
        return Version::default();
    };
    let mut version = Version::default();

    // The fixed info is what Windows compares; the string block below is what
    // it prints, and the two do not always agree.
    if let Some(fixed) = root.value().get(..FIXED_FILE_INFO_SIZE) {
        if u32_at(fixed, 0) == Some(FIXED_FILE_INFO_SIGNATURE) {
            let file = u32_at(fixed, 8).unwrap_or(0);
            let other = u32_at(fixed, 12).unwrap_or(0);
            version.numbers = Some([file >> 16, file & 0xFFFF, other >> 16, other & 0xFFFF]);
        }
    }

    for file_info in root.children() {
        if file_info.key != "StringFileInfo" {
            continue;
        }
        for language in file_info.children() {
            for item in language.children() {
                if item.key != "FileVersion" {
                    continue;
                }
                if let Some((text, _)) = utf16_at(item.bytes, item.value_at) {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        version.text = Some(text);
                    }
                }
            }
        }
    }
    version
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

    /// A version-info node: header, UTF-16 key, padded value, then children.
    fn node(key: &str, value: &[u8], value_length: usize, children: &[u8]) -> Vec<u8> {
        let key = utf16(key);
        let value_at = align4(6 + key.len());
        let length = value_at + value.len() + children.len();
        let mut bytes = Vec::with_capacity(align4(length));
        bytes.extend_from_slice(&(length as u16).to_le_bytes());
        bytes.extend_from_slice(&(value_length as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&key);
        bytes.resize(value_at, 0);
        bytes.extend_from_slice(value);
        bytes.extend_from_slice(children);
        bytes.resize(align4(length), 0);
        bytes
    }

    fn utf16(text: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    fn fixed_info(major: u32, minor: u32) -> Vec<u8> {
        let mut value = Vec::new();
        for field in [
            FIXED_FILE_INFO_SIGNATURE,
            0x0001_0000,
            (major << 16) | minor,
            0,
            (major << 16) | minor,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ] {
            value.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(value.len(), FIXED_FILE_INFO_SIZE);
        value
    }

    #[test]
    fn reads_both_the_compared_numbers_and_the_shown_string() {
        // The shape Windows writes: a fixed block to compare by and a
        // `StringFileInfo` whose `FileVersion` is what Explorer prints.
        let file_version = node("FileVersion", &utf16("96.0"), 5, &[]);
        let language = node("040904B0", &[], 0, &file_version);
        let strings = node("StringFileInfo", &[], 0, &language);
        let other = node(
            "VarFileInfo",
            &[],
            0,
            &node("Translation", &[0x04, 0x00], 0, &[]),
        );
        let root = node(
            "VS_VERSIONINFO",
            &fixed_info(96, 0),
            FIXED_FILE_INFO_SIZE,
            &[strings, other].concat(),
        );

        let version = parse_version_info(&root);
        assert_eq!(version.numbers(), Some([96, 0, 0, 0]));
        assert_eq!(version.text(), Some("96.0"));
    }

    #[test]
    fn tolerates_a_block_with_no_string_file_info() {
        let root = node(
            "VS_VERSIONINFO",
            &fixed_info(7, 1),
            FIXED_FILE_INFO_SIZE,
            &[],
        );
        let version = parse_version_info(&root);
        assert_eq!(version.numbers(), Some([7, 1, 0, 0]));
        assert_eq!(version.text(), None);
    }

    #[test]
    fn rejects_garbage_without_reading_past_it() {
        assert!(parse_version_info(&[]).is_empty());
        assert!(parse_version_info(b"not a version block at all").is_empty());

        // A node that claims bytes past the end: what is really there still
        // parses, and the padding does not become children.
        let mut dangling = node(
            "VS_VERSIONINFO",
            &fixed_info(1, 0),
            FIXED_FILE_INFO_SIZE,
            &[],
        );
        let length = u16::from_le_bytes([dangling[0], dangling[1]]) + 8;
        dangling.extend_from_slice(&[0u8; 8]);
        dangling[0..2].copy_from_slice(&length.to_le_bytes());
        let version = parse_version_info(&dangling);
        assert_eq!(version.numbers(), Some([1, 0, 0, 0]));
        assert_eq!(version.text(), None);

        // Right length, wrong signature: the numbers are not the numbers.
        let mut unsigned = node(
            "VS_VERSIONINFO",
            &fixed_info(1, 0),
            FIXED_FILE_INFO_SIZE,
            &[],
        );
        unsigned[36..40].copy_from_slice(&[0u8; 4]);
        assert!(parse_version_info(&unsigned).is_empty());
    }

    #[test]
    fn real_binaries_report_a_version() {
        // Exercises the whole resource walk, which only real files have.
        let version = version_of(Path::new(r"C:\Windows\System32\cmd.exe"));
        assert_eq!(version.numbers().is_some(), true);
        assert_eq!(
            version
                .text()
                .unwrap_or_default()
                .starts_with(char::is_numeric),
            true
        );

        // A file with no version resource at all, which is what the batch
        // table shows most often.
        assert!(version_of(Path::new(r"C:\Windows\System32\drivers\etc\hosts")).is_empty());
        assert!(version_of(Path::new(r"C:\nope\missing.exe")).is_empty());
    }
}
