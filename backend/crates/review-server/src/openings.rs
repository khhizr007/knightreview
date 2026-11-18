//! Opening book.
//!
//! Two sources, both optional:
//!
//! - The public-domain `lichess-org/chess-openings` TSV files (`eco<TAB>name<TAB>pgn`). Every
//!   position along every named line is "book"; the final position of a line carries its ECO
//!   code and name. Names always come from here.
//! - A binary key book (`BOOK_FILE`, see [`KeyBook`]): sorted Polyglot keys, each marked as a
//!   book position or a notable (named but not book) one. When loaded, it alone decides which
//!   moves are Book: the positions before and after the move must both be book positions.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use shakmaty::san::San;
use shakmaty::{Chess, Position};
use ts_rs::TS;

use crate::board;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OpeningName {
    pub eco: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct OpeningBook {
    positions: HashMap<String, Option<OpeningName>>,
    keys: Option<KeyBook>,
}

impl OpeningBook {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Uses `keys` to decide Book moves (names still come from the TSV lines).
    pub fn with_keys(mut self, keys: KeyBook) -> Self {
        self.keys = Some(keys);
        self
    }

    pub fn key_book(&self) -> Option<&KeyBook> {
        self.keys.as_ref()
    }

    /// Whether the move from `before` to `after` is a book move. With a key book, both
    /// positions must be book positions in it; otherwise the position after the move must be
    /// on a named line.
    pub fn is_book_move(&self, before: &Chess, after: &Chess) -> bool {
        match &self.keys {
            Some(keys) => {
                keys.kind(board::polyglot_key(before)) == Some(BookKind::Book)
                    && keys.kind(board::polyglot_key(after)) == Some(BookKind::Book)
            }
            None => self.contains(after),
        }
    }

    /// Loads every `*.tsv` file in `dir`. Malformed lines are skipped and counted.
    pub fn load_dir(dir: &Path) -> std::io::Result<(Self, usize)> {
        let mut book = Self::default();
        let mut skipped = 0;
        let mut files: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "tsv"))
            .collect();
        files.sort();
        for file in files {
            let text = std::fs::read_to_string(&file)?;
            skipped += book.add_tsv(&text);
        }
        Ok((book, skipped))
    }

    /// Adds TSV content; returns the number of lines that could not be used.
    pub fn add_tsv(&mut self, text: &str) -> usize {
        let mut skipped = 0;
        for line in text.lines() {
            let mut cols = line.splitn(3, '\t');
            let (Some(eco), Some(name), Some(pgn)) = (cols.next(), cols.next(), cols.next()) else {
                skipped += 1;
                continue;
            };
            if eco == "eco" {
                continue; // header
            }
            if !self.add_line(eco, name, pgn) {
                skipped += 1;
            }
        }
        skipped
    }

    fn add_line(&mut self, eco: &str, name: &str, movetext: &str) -> bool {
        let mut pos = Chess::default();
        let mut keys = Vec::new();
        for token in movetext.split_whitespace() {
            let token = token.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
            if token.is_empty() {
                continue;
            }
            let Ok(san) = token.parse::<San>() else {
                return false;
            };
            let Ok(m) = san.to_move(&pos) else {
                return false;
            };
            pos.play_unchecked(m);
            keys.push(board::epd(&pos));
        }
        let Some(last) = keys.pop() else { return false };
        for key in keys {
            self.positions.entry(key).or_insert(None);
        }
        self.positions.insert(
            last,
            Some(OpeningName {
                eco: eco.to_owned(),
                name: name.to_owned(),
            }),
        );
        true
    }

    /// Positions that can make a move Book: the key book's book positions when loaded, else
    /// the positions along the named lines.
    pub fn len(&self) -> usize {
        match &self.keys {
            Some(keys) => keys.book_positions(),
            None => self.positions.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The position is on a named line.
    pub fn contains(&self, pos: &Chess) -> bool {
        self.positions.contains_key(&board::epd(pos))
    }

    pub fn name(&self, pos: &Chess) -> Option<&OpeningName> {
        self.positions
            .get(&board::epd(pos))
            .and_then(Option::as_ref)
    }
}

/// Kind of a position in a [`KeyBook`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookKind {
    /// Moves between two book positions are Book.
    Book,
    /// A named position that is not book (traps and rare lines).
    Notable,
}

/// A binary opening book: sorted Polyglot keys, one kind bit each.
///
/// Format (little-endian), version 1:
///
/// | Offset | Size | Field |
/// |---|---|---|
/// | 0 | 4 | magic `KRBK` |
/// | 4 | 2 | version, 1 |
/// | 6 | 2 | flags, 0 (reserved) |
/// | 8 | 4 | count |
/// | 12 | 32 | source id (opaque, e.g. a SHA-256) |
/// | 44 | 8 × count | keys, `u64`, strictly ascending |
/// | 44 + 8 × count | ⌈count / 8⌉ | kinds: bit `i % 8` of byte `i / 8` is 1 for a book position, 0 for a notable one |
#[derive(Debug)]
pub struct KeyBook {
    keys: Vec<u64>,
    kinds: Vec<u8>,
    book_positions: usize,
    source: [u8; 32],
}

const KEY_BOOK_MAGIC: &[u8; 4] = b"KRBK";
const KEY_BOOK_HEADER: usize = 44;

impl KeyBook {
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < KEY_BOOK_HEADER || &data[..4] != KEY_BOOK_MAGIC {
            return Err("not a key book (bad magic)".to_owned());
        }
        let version = u16::from_le_bytes([data[4], data[5]]);
        if version != 1 {
            return Err(format!("unsupported key book version {version}"));
        }
        let count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let keys_end = count
            .checked_mul(8)
            .and_then(|n| n.checked_add(KEY_BOOK_HEADER))
            .ok_or("key count too large")?;
        let expected = keys_end + count.div_ceil(8);
        if data.len() != expected {
            return Err(format!(
                "size {} does not match {count} keys (expected {expected})",
                data.len()
            ));
        }
        let keys: Vec<u64> = data[KEY_BOOK_HEADER..keys_end]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect();
        if keys.windows(2).any(|w| w[0] >= w[1]) {
            return Err("keys are not strictly ascending".to_owned());
        }
        let kinds = data[keys_end..].to_vec();
        let book_positions = (0..count)
            .filter(|&i| (kinds[i / 8] >> (i % 8)) & 1 == 1)
            .count();
        let mut source = [0; 32];
        source.copy_from_slice(&data[12..44]);
        Ok(Self {
            keys,
            kinds,
            book_positions,
            source,
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn kind(&self, key: u64) -> Option<BookKind> {
        let i = self.keys.binary_search(&key).ok()?;
        Some(if (self.kinds[i / 8] >> (i % 8)) & 1 == 1 {
            BookKind::Book
        } else {
            BookKind::Notable
        })
    }

    /// All positions, book and notable.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn book_positions(&self) -> usize {
        self.book_positions
    }

    /// The source id from the header, as hex.
    pub fn source_hex(&self) -> String {
        self.source.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_along_lines_are_book() {
        let mut book = OpeningBook::empty();
        let skipped = book
            .add_tsv("eco\tname\tpgn\nC20\tKing's Pawn Game\t1. e4 e5\nB00\tBroken\t1. e4 Kxe8\n");
        assert_eq!(skipped, 1);
        let mut pos = Chess::default();
        pos.play_unchecked("e4".parse::<San>().unwrap().to_move(&pos).unwrap());
        assert!(book.contains(&pos));
        assert!(book.name(&pos).is_none());
        pos.play_unchecked("e5".parse::<San>().unwrap().to_move(&pos).unwrap());
        assert_eq!(book.name(&pos).map(|n| n.eco.as_str()), Some("C20"));
    }

    /// A key book file in the documented format.
    fn key_book_bytes(entries: &[(u64, bool)]) -> Vec<u8> {
        let mut sorted = entries.to_vec();
        sorted.sort();
        let mut out = b"KRBK".to_vec();
        out.extend(1u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend((sorted.len() as u32).to_le_bytes());
        out.extend([7u8; 32]);
        for (key, _) in &sorted {
            out.extend(key.to_le_bytes());
        }
        let mut bits = vec![0u8; sorted.len().div_ceil(8)];
        for (i, (_, book)) in sorted.iter().enumerate() {
            if *book {
                bits[i / 8] |= 1 << (i % 8);
            }
        }
        out.extend(bits);
        out
    }

    fn play(pos: &Chess, san: &str) -> Chess {
        let mut next = pos.clone();
        next.play_unchecked(san.parse::<San>().unwrap().to_move(pos).unwrap());
        next
    }

    #[test]
    fn key_book_reads_keys_and_kinds() {
        let entries: Vec<(u64, bool)> = (0..20u64).map(|k| (k * 3 + 1, k % 3 != 0)).collect();
        let book = KeyBook::from_bytes(&key_book_bytes(&entries)).unwrap();
        assert_eq!(book.len(), 20);
        assert_eq!(book.book_positions(), 13);
        assert_eq!(book.source_hex(), "07".repeat(32));
        for (key, is_book) in entries {
            let expected = if is_book {
                BookKind::Book
            } else {
                BookKind::Notable
            };
            assert_eq!(book.kind(key), Some(expected), "key {key}");
            assert_eq!(book.kind(key + 1), None);
        }
    }

    #[test]
    fn key_book_rejects_bad_files() {
        let good = key_book_bytes(&[(1, true), (2, false)]);
        assert!(KeyBook::from_bytes(&good).is_ok());
        let mut magic = good.clone();
        magic[0] = b'X';
        assert!(KeyBook::from_bytes(&magic).is_err());
        let mut version = good.clone();
        version[4] = 2;
        assert!(KeyBook::from_bytes(&version).is_err());
        assert!(KeyBook::from_bytes(&good[..good.len() - 1]).is_err());
        let mut longer = good.clone();
        longer.push(0);
        assert!(KeyBook::from_bytes(&longer).is_err());
        // Keys out of order.
        let mut unsorted = good.clone();
        unsorted[44..52].copy_from_slice(&5u64.to_le_bytes());
        assert!(KeyBook::from_bytes(&unsorted).is_err());
        assert!(KeyBook::from_bytes(b"KRBK").is_err());
    }

    #[test]
    fn with_a_key_book_both_positions_must_be_book() {
        let start = Chess::default();
        let e4 = play(&start, "e4");
        let e5 = play(&e4, "e5");
        let qh5 = play(&e5, "Qh5");
        let nc6 = play(&qh5, "Nc6");
        let key = |p: &Chess| board::polyglot_key(p);
        let keys = KeyBook::from_bytes(&key_book_bytes(&[
            (key(&start), true),
            (key(&e4), true),
            (key(&e5), true),
            (key(&qh5), false), // notable
            (key(&nc6), true),
        ]))
        .unwrap();
        let mut lines = OpeningBook::empty();
        lines.add_tsv("eco\tname\tpgn\nC20\tKing's Pawn Game\t1. e4 e5 2. Qh5 Nc6\n");
        assert!(
            lines.is_book_move(&e5, &qh5),
            "named lines alone: Qh5 is Book"
        );
        let book = lines.with_keys(keys);
        assert_eq!(book.len(), 4);
        assert!(book.is_book_move(&start, &e4));
        assert!(book.is_book_move(&e4, &e5));
        assert!(
            !book.is_book_move(&e5, &qh5),
            "the position after is notable"
        );
        assert!(
            !book.is_book_move(&qh5, &nc6),
            "the position before is notable"
        );
        let d4 = play(&start, "d4");
        assert!(!book.is_book_move(&start, &d4), "not in the key book");
        // Names still come from the lines.
        assert_eq!(book.name(&nc6).map(|n| n.eco.as_str()), Some("C20"));
    }
}
