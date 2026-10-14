//! Building a library image: the key that names it, and its file.

use crate::image::{ENTRY, HEADER, MAGIC, Source, Spec};
use rustc_hash::{FxHashSet, FxHasher};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::Path;
use zip::ZipArchive;

/// Identifies what `spec` puts in an image: the program's version, the tag, the derived files'
/// names, the jars by path, size and modification time, and built-in jars by name and contents.
pub fn key(spec: &Spec) -> io::Result<u64> {
    let mut h = FxHasher::default();
    (MAGIC, env!("CARGO_PKG_VERSION"), spec.tag).hash(&mut h);
    spec.derived.iter().for_each(|(name, _)| name.hash(&mut h));
    for source in spec.sources {
        match source {
            Source::File(jar) => {
                let meta = std::fs::metadata(jar)?;
                (std::fs::canonicalize(jar)?, meta.len(), meta.modified()?).hash(&mut h);
            }
            Source::Bytes(name, bytes) => (name, bytes).hash(&mut h),
        }
    }
    Ok(h.finish())
}

/// Writes the image of `spec` to `path`, in directory `dir`, through a staging file only this
/// thread writes and that is removed if the build fails.
pub fn build(spec: &Spec, dir: &Path, path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut thread = FxHasher::default();
    std::thread::current().id().hash(&mut thread);
    let tmp = path.with_extension(format!("tmp{}{:06}", std::process::id(), thread.finish() % 1_000_000));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let written = write(spec, file);
    let result = written.and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes the image of `spec` to `out_file`.
fn write(spec: &Spec, mut out_file: File) -> io::Result<()> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen = FxHashSet::default();
    for source in spec.sources {
        match source {
            Source::File(jar) => {
                collect(ZipArchive::new(File::open(jar)?), spec.keep, &mut seen, &mut files)?
            }
            Source::Bytes(_, bytes) => {
                collect(ZipArchive::new(io::Cursor::new(*bytes)), spec.keep, &mut seen, &mut files)?
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    let derived: Vec<(String, Vec<u8>)> = spec
        .derived
        .iter()
        .filter(|(name, _)| !seen.contains(*name))
        .map(|(name, f)| (name.to_string(), f(&names)))
        .collect();
    files.extend(derived);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let extras: Vec<Vec<u8>> = files.iter().map(|(name, data)| (spec.extra)(name, data)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    let mut at = HEADER + files.len() * ENTRY;
    let mut blobs = Vec::new();
    for ((name, data), extra) in files.iter().zip(&extras) {
        for part in [name.as_bytes(), data.as_slice(), extra.as_slice()] {
            out.extend_from_slice(&(at as u64).to_le_bytes());
            out.extend_from_slice(&(part.len() as u64).to_le_bytes());
            at += part.len();
            blobs.push(part);
        }
    }
    out_file.write_all(&out)?;
    blobs.iter().try_for_each(|b| out_file.write_all(b))?;
    out_file.sync_all()
}

/// Adds the files of `zip` that `keep` accepts and `seen` does not hold yet to `files`.
fn collect<R: Read + io::Seek>(
    zip: zip::result::ZipResult<ZipArchive<R>>,
    keep: fn(&str) -> bool,
    seen: &mut FxHashSet<String>,
    files: &mut Vec<(String, Vec<u8>)>,
) -> io::Result<()> {
    let mut zip = zip.map_err(io::Error::other)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(io::Error::other)?;
        let name = file.name().to_owned();
        if !keep(&name) || !seen.insert(name.clone()) {
            continue;
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)?;
        files.push((name, data));
    }
    Ok(())
}
