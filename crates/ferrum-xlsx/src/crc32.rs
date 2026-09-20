//! CRC-32, which every entry in a ZIP archive carries.
//!
//! The ordinary IEEE polynomial, reflected, as used by ZIP, PNG and gzip. The
//! table is built once on first use rather than written out, because 256
//! constants in the source are 256 chances to mistype one.

use std::sync::OnceLock;

/// The reflected form of the IEEE 802.3 polynomial.
const POLYNOMIAL: u32 = 0xEDB8_8320;

fn table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        let mut index = 0usize;
        while index < 256 {
            let mut value = index as u32;
            let mut bit = 0;
            while bit < 8 {
                value = if value & 1 == 1 {
                    (value >> 1) ^ POLYNOMIAL
                } else {
                    value >> 1
                };
                bit += 1;
            }
            table[index] = value;
            index += 1;
        }
        table
    })
}

/// A running CRC-32, for data that arrives in pieces.
#[derive(Clone, Copy, Debug)]
pub struct Crc32 {
    state: u32,
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    pub const fn new() -> Self {
        Self { state: !0 }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        let table = table();
        let mut state = self.state;
        for byte in bytes {
            let index = ((state ^ u32::from(*byte)) & 0xFF) as usize;
            state = (state >> 8) ^ table[index];
        }
        self.state = state;
    }

    pub const fn finish(self) -> u32 {
        !self.state
    }
}

/// The CRC-32 of a whole slice.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut sum = Crc32::new();
    sum.update(bytes);
    sum.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_check_value_matches() {
        // Every CRC-32 implementation is expected to produce this for the
        // nine ASCII digits, which is what makes it the standard check.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_empty_input_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_few_known_values() {
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"abc"), 0x3524_41C2);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn feeding_it_in_pieces_gives_the_same_answer() {
        let whole = crc32(b"the quick brown fox");
        let mut running = Crc32::new();
        running.update(b"the qui");
        running.update(b"ck bro");
        running.update(b"wn fox");
        assert_eq!(running.finish(), whole);
    }

    #[test]
    fn an_empty_piece_changes_nothing() {
        let mut running = Crc32::new();
        running.update(b"abc");
        running.update(b"");
        assert_eq!(running.finish(), crc32(b"abc"));
    }

    #[test]
    fn it_is_sensitive_to_order() {
        assert_ne!(crc32(b"ab"), crc32(b"ba"));
    }
}
