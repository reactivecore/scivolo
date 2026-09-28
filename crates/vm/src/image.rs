//! The library image: every `.nir` file of a list of jars, uncompressed in one file with a sorted
//! index and each file's member table (`member_table`), built once per set of jar versions and
//! mapped read-only afterwards.
//!
//! Layout, little-endian: `MAGIC`, `u32` entry count, then per entry `u64` offset and length of
//! its name, its data and its member table (empty if none), sorted by name; then the contents.

use rustc_hash::FxHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAGIC: &[u8; 8] = b"SCALAI02";
const ENTRY: usize = 48;
const HEADER: usize = 12;

pub struct Image {
    map: *const u8,
    len: usize,
    count: usize,
}

impl Image {
    /// The image of `jars` in directory `cache`, built there first if missing.
    pub fn open(jars: &[PathBuf], cache: &Path) -> io::Result<Self> {
        let path = cache.join(format!("libs-{:016x}.img", key(jars)?));
        if path.exists()
            && let Ok(image) = Self::map(&path)
        {
            crate::cache::touch(&path);
            return Ok(image);
        }
        build(jars, cache, &path)?;
        crate::cache::prune(cache, crate::cache::UNUSED, |n| {
            n.strip_prefix("libs-").and_then(|n| n.strip_suffix(".img")).is_some_and(crate::cache::is_key)
        });
        crate::cache::prune(cache, crate::cache::ABANDONED, |n| crate::cache::is_staging(n, "libs-", ".tmp"));
        Self::map(&path)
    }

    fn map(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        if len < HEADER {
            return Err(io::Error::other("library image too short"));
        }
        use std::os::fd::AsRawFd;
        // SAFETY: a read-only private mapping of a file this process only reads; the mapping
        // stays valid after `file` is closed and is unmapped only in `drop`.
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
        self.bytes().get(start..start + len).unwrap_or(&[])
    }

    fn name(&self, entry: usize) -> &[u8] {
        self.slice(entry, 0)
    }

    /// The bytes of file `rel`, e.g. `scala/Predef$.nir`, and its member table if it has one.
    pub fn find(&self, rel: &str) -> Option<(&[u8], Option<&[u8]>)> {
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let mid = (low + high) / 2;
            match self.name(mid).cmp(rel.as_bytes()) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    let table = self.slice(mid, 2);
                    return Some((self.slice(mid, 1), Some(table).filter(|t| !t.is_empty())));
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

/// Identifies the jars' contents by path, size and modification time.
fn key(jars: &[PathBuf]) -> io::Result<u64> {
    let mut h = FxHasher::default();
    MAGIC.hash(&mut h);
    for jar in jars {
        let meta = std::fs::metadata(jar)?;
        (std::fs::canonicalize(jar)?, meta.len(), meta.modified()?).hash(&mut h);
    }
    Ok(h.finish())
}

/// Writes the image of `jars` to `path`; a jar's file hides the same file in later jars.
fn build(jars: &[PathBuf], cache: &Path, path: &Path) -> io::Result<()> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    for jar in jars {
        let mut zip = ZipArchive::new(File::open(jar)?).map_err(io::Error::other)?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(io::Error::other)?;
            let name = file.name().to_owned();
            if !name.ends_with(".nir") || !seen.insert(name.clone()) {
                continue;
            }
            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data)?;
            files.push((name, data));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let tables: Vec<Vec<u8>> = files
        .iter()
        .map(|(name, data)| {
            let top = name.strip_suffix(".nir").unwrap_or(name).replace('/', ".");
            crate::member_table::build(data, &top).unwrap_or_default()
        })
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    let mut at = HEADER + files.len() * ENTRY;
    let mut blobs = Vec::new();
    for ((name, data), table) in files.iter().zip(&tables) {
        for part in [name.as_bytes(), data.as_slice(), table.as_slice()] {
            out.extend_from_slice(&(at as u64).to_le_bytes());
            out.extend_from_slice(&(part.len() as u64).to_le_bytes());
            at += part.len();
            blobs.push(part);
        }
    }
    std::fs::create_dir_all(cache)?;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let mut f = File::create(&tmp)?;
    f.write_all(&out)?;
    blobs.iter().try_for_each(|b| f.write_all(b))?;
    drop(f);
    std::fs::rename(&tmp, path)
}
