//! The ZIP container a spreadsheet file is packed in.
//!
//! Only the parts the format actually uses. Entries are **stored**, not
//! compressed: a stored ZIP is a valid ZIP that every reader accepts, and it
//! costs a DEFLATE encoder that would otherwise have to come first. Reading
//! files written elsewhere does need DEFLATE, and that is the next piece.
//!
//! The layout, from the appnote:
//!
//! ```text
//! local header + data      one per entry
//! ...
//! central directory        one record per entry
//! end of central directory
//! ```

use crate::crc32::crc32;

const LOCAL_HEADER: u32 = 0x0403_4B50;
const CENTRAL_HEADER: u32 = 0x0201_4B50;
const END_OF_DIRECTORY: u32 = 0x0605_4B50;

/// The version that understands stored entries and directories.
const VERSION: u16 = 20;

/// Stored, meaning the bytes are written as they are.
pub const METHOD_STORED: u16 = 0;

/// Deflated. Recognised when reading; not produced.
pub const METHOD_DEFLATED: u16 = 8;

/// Why a file could not be packed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ZipError {
    /// An entry, or the archive, is larger than a plain ZIP can address.
    ///
    /// Past this the format needs its 64-bit extension, which a spreadsheet
    /// of any sane size never reaches.
    TooLarge,
    /// A name that cannot go in an archive.
    BadName(String),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => f.write_str("the archive is too large for this format"),
            Self::BadName(name) => write!(f, "{name:?} cannot be an entry name"),
        }
    }
}

impl std::error::Error for ZipError {}

/// A moment, in the shape MS-DOS used and ZIP inherited.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DosTime {
    time: u16,
    date: u16,
}

impl DosTime {
    /// The earliest the format can express, which is what an archive with no
    /// meaningful timestamp should carry.
    ///
    /// Using a fixed value by default makes the output byte-for-byte
    /// reproducible, so two saves of one workbook compare equal.
    pub const EPOCH: Self = Self {
        time: 0,
        date: 1 << 5 | 1, // January, the first
    };

    /// Build from a civil date and time. Out-of-range parts are clamped
    /// rather than refused, because a timestamp is not worth failing a save
    /// over.
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        let year = year.clamp(1980, 2107) - 1980;
        let month = u16::from(month.clamp(1, 12));
        let day = u16::from(day.clamp(1, 31));
        let hour = u16::from(hour.min(23));
        let minute = u16::from(minute.min(59));
        // The format keeps seconds in units of two.
        let second = u16::from(second.min(59)) / 2;
        Self {
            time: hour << 11 | minute << 5 | second,
            date: year << 9 | month << 5 | day,
        }
    }
}

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Packs entries into an archive, in memory.
///
/// A spreadsheet is a few hundred kilobytes of XML, so streaming to disk
/// would buy nothing and cost the ability to hand the result straight to a
/// test.
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Entry>,
    time: DosTime,
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipWriter {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            entries: Vec::new(),
            time: DosTime::EPOCH,
        }
    }

    /// Stamp every entry with this time. Defaults to [`DosTime::EPOCH`], which
    /// keeps the output reproducible.
    pub fn with_time(mut self, time: DosTime) -> Self {
        self.time = time;
        self
    }

    /// Add one file.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), ZipError> {
        check_name(name)?;
        let size = u32::try_from(data.len()).map_err(|_| ZipError::TooLarge)?;
        let offset = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;
        let crc = crc32(data);

        self.out.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        self.out.extend_from_slice(&VERSION.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.out.extend_from_slice(&METHOD_STORED.to_le_bytes());
        self.out.extend_from_slice(&self.time.time.to_le_bytes());
        self.out.extend_from_slice(&self.time.date.to_le_bytes());
        self.out.extend_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes()); // compressed
        self.out.extend_from_slice(&size.to_le_bytes()); // uncompressed
        self.out
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // extra
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(data);

        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Close the archive and hand back its bytes.
    pub fn finish(mut self) -> Result<Vec<u8>, ZipError> {
        let directory_start = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;

        for entry in &self.entries {
            self.out.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
            self.out.extend_from_slice(&VERSION.to_le_bytes()); // made by
            self.out.extend_from_slice(&VERSION.to_le_bytes()); // needed
            self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
            self.out.extend_from_slice(&METHOD_STORED.to_le_bytes());
            self.out.extend_from_slice(&self.time.time.to_le_bytes());
            self.out.extend_from_slice(&self.time.date.to_le_bytes());
            self.out.extend_from_slice(&entry.crc.to_le_bytes());
            self.out.extend_from_slice(&entry.size.to_le_bytes());
            self.out.extend_from_slice(&entry.size.to_le_bytes());
            self.out
                .extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes()); // extra
            self.out.extend_from_slice(&0u16.to_le_bytes()); // comment
            self.out.extend_from_slice(&0u16.to_le_bytes()); // disk
            self.out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            self.out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            self.out.extend_from_slice(&entry.offset.to_le_bytes());
            self.out.extend_from_slice(entry.name.as_bytes());
        }

        let directory_end = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;
        let count = u16::try_from(self.entries.len()).map_err(|_| ZipError::TooLarge)?;

        self.out.extend_from_slice(&END_OF_DIRECTORY.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        self.out.extend_from_slice(&0u16.to_le_bytes()); // disk with directory
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out
            .extend_from_slice(&(directory_end - directory_start).to_le_bytes());
        self.out.extend_from_slice(&directory_start.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // comment

        Ok(self.out)
    }
}

/// Entry names are forward-slashed relative paths, and must not try to climb
/// out of the archive when it is unpacked.
fn check_name(name: &str) -> Result<(), ZipError> {
    let bad = name.is_empty()
        || name.len() > u16::MAX as usize
        || name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|part| part == "..");
    if bad {
        return Err(ZipError::BadName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    #[test]
    fn an_empty_archive_is_just_its_end_record() {
        let bytes = ZipWriter::new().finish().unwrap();
        assert_eq!(bytes.len(), 22);
        assert_eq!(read_u32(&bytes, 0), END_OF_DIRECTORY);
        assert_eq!(read_u16(&bytes, 10), 0, "no entries");
    }

    #[test]
    fn one_entry_produces_a_local_header_and_a_directory_record() {
        let mut zip = ZipWriter::new();
        zip.add("hello.txt", b"hello").unwrap();
        let bytes = zip.finish().unwrap();

        assert_eq!(read_u32(&bytes, 0), LOCAL_HEADER);
        assert_eq!(read_u16(&bytes, 8), METHOD_STORED);
        assert_eq!(read_u32(&bytes, 14), crc32(b"hello"));
        assert_eq!(read_u32(&bytes, 18), 5, "compressed size");
        assert_eq!(read_u32(&bytes, 22), 5, "uncompressed size");
        assert_eq!(read_u16(&bytes, 26), 9, "name length");
        assert_eq!(&bytes[30..39], b"hello.txt");
        assert_eq!(&bytes[39..44], b"hello");

        // The end record is the last 22 bytes and points at the directory.
        let end = bytes.len() - 22;
        assert_eq!(read_u32(&bytes, end), END_OF_DIRECTORY);
        assert_eq!(read_u16(&bytes, end + 10), 1);
        let directory_at = read_u32(&bytes, end + 16) as usize;
        assert_eq!(read_u32(&bytes, directory_at), CENTRAL_HEADER);
    }

    #[test]
    fn the_directory_points_at_each_local_header() {
        let mut zip = ZipWriter::new();
        zip.add("one.txt", b"first").unwrap();
        zip.add("two.txt", b"second entry").unwrap();
        zip.add("dir/three.txt", b"").unwrap();
        let bytes = zip.finish().unwrap();

        let end = bytes.len() - 22;
        assert_eq!(read_u16(&bytes, end + 10), 3);
        let mut at = read_u32(&bytes, end + 16) as usize;

        for (name, data) in [
            ("one.txt", &b"first"[..]),
            ("two.txt", &b"second entry"[..]),
            ("dir/three.txt", &b""[..]),
        ] {
            assert_eq!(read_u32(&bytes, at), CENTRAL_HEADER);
            let name_len = read_u16(&bytes, at + 28) as usize;
            assert_eq!(&bytes[at + 46..at + 46 + name_len], name.as_bytes());

            // Follow the offset and check the same entry is there.
            let local = read_u32(&bytes, at + 42) as usize;
            assert_eq!(read_u32(&bytes, local), LOCAL_HEADER);
            let local_name_len = read_u16(&bytes, local + 26) as usize;
            let data_at = local + 30 + local_name_len;
            assert_eq!(&bytes[data_at..data_at + data.len()], data);

            at += 46 + name_len;
        }
    }

    #[test]
    fn the_output_is_reproducible() {
        let build = || {
            let mut zip = ZipWriter::new();
            zip.add("a.xml", b"<a/>").unwrap();
            zip.add("b.xml", b"<b/>").unwrap();
            zip.finish().unwrap()
        };
        assert_eq!(build(), build(), "two saves should compare equal");
    }

    #[test]
    fn a_timestamp_packs_into_the_dos_fields() {
        // 2026-09-20 14:30:44
        let stamp = DosTime::new(2026, 9, 20, 14, 30, 44);
        assert_eq!(stamp.date, (2026 - 1980) << 9 | 9 << 5 | 20);
        assert_eq!(stamp.time, 14 << 11 | 30 << 5 | 22);
    }

    #[test]
    fn an_out_of_range_timestamp_is_clamped_rather_than_refused() {
        // A save should not fail because a clock is wrong.
        let early = DosTime::new(1900, 0, 0, 99, 99, 99);
        assert_eq!(early.date, 1 << 5 | 1);
        let late = DosTime::new(3000, 13, 40, 25, 61, 61);
        assert_eq!(late.date, (2107 - 1980) << 9 | 12 << 5 | 31);
    }

    #[test]
    fn dangerous_names_are_refused() {
        let mut zip = ZipWriter::new();
        assert!(zip.add("", b"x").is_err());
        assert!(zip.add("/absolute", b"x").is_err());
        assert!(zip.add("..", b"x").is_err());
        assert!(zip.add("a/../../escape", b"x").is_err());
        assert!(zip.add("back\\slash", b"x").is_err());
        // And a reasonable one is fine.
        assert!(zip.add("xl/worksheets/sheet1.xml", b"x").is_ok());
    }

    #[test]
    fn an_empty_entry_is_allowed() {
        let mut zip = ZipWriter::new();
        zip.add("empty", b"").unwrap();
        let bytes = zip.finish().unwrap();
        assert_eq!(read_u32(&bytes, 14), 0, "the crc of nothing is zero");
        assert_eq!(read_u32(&bytes, 18), 0);
    }
}
