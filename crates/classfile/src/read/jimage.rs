//! A JDK's run-time image `lib/modules` (jimage, OpenJDK's `BasicImageReader` and
//! `ImageLocation`): its uncompressed class files by internal name.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: u32 = 0xCAFE_DADA;
const HEADER: u64 = 28;
/// The kinds of a location's attributes.
const PARENT: usize = 2;
const BASE: usize = 3;
const EXTENSION: usize = 4;
const OFFSET: usize = 5;
const COMPRESSED: usize = 6;
const SIZE: usize = 7;

pub struct Jimage {
    file: File,
    /// Where the resources start.
    resources: u64,
    /// The offset and size of each class's file among the resources.
    classes: HashMap<String, (u64, u64)>,
}

impl Jimage {
    /// The image at `path`; none if it is none or lacks an uncompressed `java/lang/Object`.
    pub fn open(path: &Path) -> Option<Jimage> {
        let mut file = File::open(path).ok()?;
        let mut header = [0; HEADER as usize];
        file.read_exact(&mut header).ok()?;
        let little = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) == MAGIC;
        let word = |&b: &[u8; 4]| if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
        let words: Vec<usize> = header.as_chunks().0.iter().map(|w| word(w) as usize).collect();
        if words[0] != MAGIC as usize {
            return None;
        }
        let (table, locations, strings) = (words[4], words[5], words[6]);
        let mut index = vec![0; table * 8 + locations + strings];
        file.read_exact(&mut index).ok()?;
        let (offsets, rest) = index[table * 4..].split_at(table * 4);
        let (locations, strings) = rest.split_at(locations);
        let mut classes = HashMap::new();
        for offset in offsets.as_chunks().0 {
            let a = attributes(locations.get(word(offset) as usize..)?);
            if a[COMPRESSED] != 0 || text(strings, a[EXTENSION]) != Some("class") {
                continue;
            }
            let (parent, base) = (text(strings, a[PARENT])?, text(strings, a[BASE])?);
            let name = if parent.is_empty() { base.to_owned() } else { format!("{parent}/{base}") };
            classes.insert(name, (a[OFFSET], a[SIZE]));
        }
        classes.contains_key("java/lang/Object").then_some(Jimage {
            file,
            resources: HEADER + index.len() as u64,
            classes,
        })
    }

    /// The bytes of the class file of class `name`.
    pub fn bytes(&mut self, name: &str) -> Option<Vec<u8>> {
        let &(offset, size) = self.classes.get(name)?;
        self.file.seek(SeekFrom::Start(self.resources + offset)).ok()?;
        let mut bytes = vec![0; usize::try_from(size).ok()?];
        self.file.read_exact(&mut bytes).ok()?;
        Some(bytes)
    }
}

/// The attributes of the location `bytes` start with, by kind.
fn attributes(bytes: &[u8]) -> [u64; 8] {
    let mut a = [0; 8];
    let mut at = 0;
    while let Some(&byte) = bytes.get(at)
        && let Some(slot) = a.get_mut(usize::from(byte >> 3)).filter(|_| byte > 7)
        && let Some(value) = bytes.get(at + 1..at + 2 + usize::from(byte & 7))
    {
        *slot = value.iter().fold(0, |v, &b| v << 8 | u64::from(b));
        at += 2 + usize::from(byte & 7);
    }
    a
}

/// The text at `offset` of the strings, up to its NUL.
fn text(strings: &[u8], offset: u64) -> Option<&str> {
    let s = strings.get(usize::try_from(offset).ok()?..)?;
    std::str::from_utf8(&s[..s.iter().position(|&b| b == 0)?]).ok()
}
