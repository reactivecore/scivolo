//! A JDK's run-time image `lib/modules` (jimage, OpenJDK's `BasicImageReader` and
//! `ImageLocation`): its uncompressed class files by internal name.

use hashbrown::HashTable;
use hashbrown::hash_table::Entry;
use rustc_hash::FxHasher;
use std::fs::File;
use std::hash::Hasher;
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
    /// The image's string table.
    strings: Vec<u8>,
    /// The class files, found by their internal names.
    classes: HashTable<Class>,
}

/// A class file of the image: where its package and simple name start in the string table, and
/// its offset and size among the resources.
struct Class {
    parent: u32,
    base: u32,
    offset: u64,
    size: u64,
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
        let resources = HEADER + index.len() as u64;
        let strings = index.split_off(table * 8 + locations);
        let (offsets, locations) = index[table * 4..].split_at(table * 4);
        let mut classes = HashTable::with_capacity(table);
        for offset in offsets.as_chunks().0 {
            let a = attributes(locations.get(word(offset) as usize..)?);
            if a[COMPRESSED] != 0 || text(&strings, a[EXTENSION]) != Some(b"class") {
                continue;
            }
            let (parent, base) = (text(&strings, a[PARENT])?, text(&strings, a[BASE])?);
            std::str::from_utf8(parent).ok()?;
            std::str::from_utf8(base).ok()?;
            let class = Class {
                parent: u32::try_from(a[PARENT]).ok()?,
                base: u32::try_from(a[BASE]).ok()?,
                offset: a[OFFSET],
                size: a[SIZE],
            };
            let is = |c: &Class| is_named(&strings, c, parent, base);
            match classes.entry(hash(parent, base), is, |c| hash_of(&strings, c)) {
                Entry::Occupied(mut e) => *e.get_mut() = class,
                Entry::Vacant(e) => _ = e.insert(class),
            }
        }
        let image = Jimage { file, resources, strings, classes };
        image.find("java/lang/Object").is_some().then_some(image)
    }

    /// The internal names of the packages of the classes.
    pub fn packages(&self) -> Vec<String> {
        let mut parents: Vec<u32> = self.classes.iter().map(|c| c.parent).collect();
        parents.sort_unstable();
        parents.dedup();
        let names = parents.into_iter().filter_map(|p| text(&self.strings, p.into()));
        names.map(|n| String::from_utf8_lossy(n).into_owned()).collect()
    }

    /// The bytes of the class file of class `name`.
    pub fn bytes(&mut self, name: &str) -> Option<Vec<u8>> {
        let &Class { offset, size, .. } = self.find(name)?;
        self.file.seek(SeekFrom::Start(self.resources + offset)).ok()?;
        let mut bytes = vec![0; usize::try_from(size).ok()?];
        self.file.read_exact(&mut bytes).ok()?;
        Some(bytes)
    }

    /// The class of internal name `name`.
    fn find(&self, name: &str) -> Option<&Class> {
        let (parent, base) = name.rsplit_once('/').unwrap_or(("", name));
        let (parent, base) = (parent.as_bytes(), base.as_bytes());
        self.classes.find(hash(parent, base), |c| is_named(&self.strings, c, parent, base))
    }
}

/// Whether class `c` is of package `parent` and simple name `base`.
fn is_named(strings: &[u8], c: &Class, parent: &[u8], base: &[u8]) -> bool {
    text(strings, c.parent.into()) == Some(parent) && text(strings, c.base.into()) == Some(base)
}

/// The hash of the internal name of package `parent` and simple name `base`.
fn hash(parent: &[u8], base: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    h.write(parent);
    h.write_u8(0);
    h.write(base);
    h.finish()
}

fn hash_of(strings: &[u8], c: &Class) -> u64 {
    let text = |at: u32| text(strings, at.into()).unwrap_or_default();
    hash(text(c.parent), text(c.base))
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
fn text(strings: &[u8], offset: u64) -> Option<&[u8]> {
    let s = strings.get(usize::try_from(offset).ok()?..)?;
    Some(&s[..s.iter().position(|&b| b == 0)?])
}
