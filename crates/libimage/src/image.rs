//! The library image: the files of a list of jars that a filter keeps, uncompressed in one file
//! with a sorted index and an extra blob per file, built once per set of jar versions and
//! mapped read-only afterwards.
//!
//! Layout, little-endian: `MAGIC`, `u32` entry count, then per entry `u64` offset and length of
//! its name, its data and its extra blob (empty if none), sorted by name; then the contents.

use crate::build::{build, key};
use crate::cache;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"SCALAI03";
pub const ENTRY: usize = 48;
pub const HEADER: usize = 12;

/// A jar the image takes files from.
pub enum Source {
    File(PathBuf),
    /// A jar built into the program, by name and contents.
    Bytes(&'static str, &'static [u8]),
}

/// A file an image derives from the sorted names of its other files.
pub struct Derived<'a> {
    pub name: &'a str,
    /// What `contents` depends on besides the names; it changes with the file's format.
    pub version: &'a str,
    pub contents: &'a dyn Fn(&[&str]) -> Vec<u8>,
}

/// What an image holds: the files of `sources` whose names `keep` accepts, the first of equal
/// names winning, each with the blob `extra` computes, and the `derived` files. `tag` names what
/// `keep` and `extra` do, and changes when they do.
pub struct Spec<'a> {
    pub sources: &'a [Source],
    pub keep: fn(&str) -> bool,
    pub extra: &'a dyn Fn(&str, &[u8]) -> Vec<u8>,
    pub derived: &'a [Derived<'a>],
    pub tag: &'a str,
}

pub struct Image {
    map: *const u8,
    len: usize,
    count: usize,
}

// SAFETY: the mapping is read-only, image files are never written after they are renamed into
// place, and it is unmapped only when the image is dropped; any thread may read it.
unsafe impl Send for Image {}
// SAFETY: as for `Send`: shared references only read the mapping.
unsafe impl Sync for Image {}

impl Image {
    /// The image of `spec` in directory `dir`, built there first if missing.
    pub fn open(spec: &Spec, dir: &Path) -> io::Result<Self> {
        let path = dir.join(format!("libs-{:016x}.img", key(spec)?));
        if path.exists()
            && let Ok(image) = Self::map(&path)
        {
            cache::touch(&path);
            return Ok(image);
        }
        cache::prune(dir, cache::ABANDONED, |n| cache::is_staging(n, "libs-", ".tmp"));
        build(spec, dir, &path)?;
        cache::prune(dir, cache::UNUSED, |n| {
            n.strip_prefix("libs-").and_then(|n| n.strip_suffix(".img")).is_some_and(cache::is_key)
        });
        Self::map(&path)
    }

    fn map(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        if len < HEADER {
            return Err(io::Error::other("library image too short"));
        }
        use std::os::fd::AsRawFd;
        // SAFETY: a read-only private mapping of an image file, which is never written after it
        // is renamed into place; the mapping stays valid after `file` is closed and is unmapped
        // only in `drop`.
        let map = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, file.as_raw_fd(), 0)
        };
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let mut image = Self { map: map as *const u8, len, count: 0 };
        let bytes = image.bytes();
        if &bytes[..8] != MAGIC {
            return Err(io::Error::other("not a library image"));
        }
        image.count = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
        if HEADER + image.count * ENTRY > len {
            return Err(io::Error::other("library image index truncated"));
        }
        let end = match image.count.checked_sub(1) {
            Some(last) => {
                let at = HEADER + last * ENTRY + 2 * 16;
                image.word(at).checked_add(image.word(at + 8))
            }
            None => Some(HEADER),
        };
        if end != Some(len) {
            return Err(io::Error::other("library image contents truncated"));
        }
        Ok(image)
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: `map` points to `len` mapped, readable bytes for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.map, self.len) }
    }

    fn word(&self, at: usize) -> usize {
        u64::from_le_bytes(self.bytes()[at..at + 8].try_into().expect("8 bytes")) as usize
    }

    fn slice(&self, entry: usize, field: usize) -> &[u8] {
        let at = HEADER + entry * ENTRY + field * 16;
        let (start, len) = (self.word(at), self.word(at + 8));
        start.checked_add(len).and_then(|end| self.bytes().get(start..end)).unwrap_or(&[])
    }

    fn name(&self, entry: usize) -> &[u8] {
        self.slice(entry, 0)
    }

    /// The bytes of file `rel`, e.g. `scala/Predef$.nir`, and its extra blob if not empty.
    pub fn find(&self, rel: &str) -> Option<(&[u8], Option<&[u8]>)> {
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let mid = (low + high) / 2;
            match self.name(mid).cmp(rel.as_bytes()) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    let extra = self.slice(mid, 2);
                    return Some((self.slice(mid, 1), Some(extra).filter(|t| !t.is_empty())));
                }
            }
        }
        None
    }

    /// The names of all files, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        (0..self.count).filter_map(|i| std::str::from_utf8(self.name(i)).ok())
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: `map` and `len` describe the mapping `map` created; nothing refers into it once
        // the image is dropped, as `find` and `names` borrow `self`.
        unsafe { libc::munmap(self.map as *mut libc::c_void, self.len) };
    }
}
