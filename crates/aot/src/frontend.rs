//! Scala sources to NIR through the own frontend, typed together as one unit, cached by the
//! sources, the library jars and the build.

use crate::R;
use libimage::{Image, cache};
use libs::{BUILD, NATIVE, SCALA, cache_dir};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::source::{SourceFile, Sources};

/// The NIR directory of `files`, typed against `image` of the library jars `jars`, and their
/// main classes, compiled unless cached. A compile goes to a directory of its own, renamed into
/// place when complete.
pub fn compile(files: &[PathBuf], jars: &[PathBuf], image: &'static Image) -> R<(PathBuf, Vec<String>)> {
    let sources = read(files)?;
    let units = cache_dir().join("aot-units");
    let dir = units.join(format!("{:016x}", key(files, &sources, jars)));
    cache::touch(&dir);
    if let Some(hit) = cached(&dir) {
        return Ok(hit);
    }
    if cache::prune_due(&units) {
        cache::prune(&units, cache::UNUSED, cache::is_key);
        cache::prune(&units, cache::ABANDONED, |n| cache::is_staging(n, "", "."));
    }
    let staging = dir.with_extension(std::process::id().to_string());
    let built = build(&sources, image, &staging.join("classes")).and_then(|mains| {
        std::fs::write(staging.join("main"), mains.join("\n"))
            .map_err(|e| format!("{}: {e}", staging.display()))
    });
    if built.is_ok() && std::fs::rename(&staging, &dir).is_ok() {
        return cached(&dir).ok_or_else(|| format!("{}: unreadable", dir.display()));
    }
    let _ = std::fs::remove_dir_all(&staging);
    built?;
    cached(&dir).ok_or_else(|| format!("{}: incomplete; remove it", dir.display()))
}

/// Compiles `files` against `image` into the NIR directory `classes`, uncached.
pub fn compile_into(files: &[PathBuf], image: &'static Image, classes: &Path) -> R<()> {
    build(&read(files)?, image, classes).map(drop)
}

fn read(files: &[PathBuf]) -> R<Vec<SourceFile>> {
    let read = |f: &PathBuf| {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let name = f.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        Ok(SourceFile::new(name, text))
    };
    files.iter().map(read).collect()
}

/// The cache key of `sources` read from `files` and compiled against `jars`.
// Temporary until #115 (per-file NIR reuse): one key for all sources, so a change to one
// recompiles all.
fn key(files: &[PathBuf], sources: &[SourceFile], jars: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    (BUILD, SCALA, NATIVE).hash(&mut h);
    for (file, source) in files.iter().zip(sources) {
        (file.canonicalize().unwrap_or_else(|_| file.clone()), &source.text).hash(&mut h);
    }
    for jar in jars {
        let meta = std::fs::metadata(jar).ok();
        (jar, meta.as_ref().map(|m| m.len()), meta.and_then(|m| m.modified().ok())).hash(&mut h);
    }
    h.finish()
}

/// The classes directory and main classes of a complete cache entry.
fn cached(dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    let mains = std::fs::read_to_string(dir.join("main")).ok()?;
    Some((dir.join("classes"), mains.lines().map(str::to_owned).collect()))
}

/// Writes the NIR files of `sources` under `classes` and returns their main classes, sorted.
fn build(sources: &[SourceFile], image: &'static Image, classes: &Path) -> R<Vec<String>> {
    let cfg = Config::from_options(&[]);
    let compiled = catch_unwind(AssertUnwindSafe(|| {
        lower::compile_sources(&mut tasty::Universe::open_image(image), sources, cfg)
    }));
    let (files, warnings) = compiled
        .map_err(|p| format!("the own frontend failed: {}", panic_text(p.as_ref())))?
        .map_err(|r| format!("the own frontend refuses the sources: {}", r.what))?;
    let located = Sources::new(sources);
    for w in &warnings.items {
        let at = w.offset.map_or_else(String::new, |o| format!("{}: ", lower::position(&located, o)));
        eprintln!("warning: {at}{}", w.message);
    }
    let mut mains = Vec::new();
    for f in files {
        let path = classes.join(format!("{}.nir", f.name.replace('.', "/")));
        let dir = path.parent().unwrap_or(classes);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        mains
            .extend(nir::mains::defined(&f.bytes).map_err(|e| format!("{}: undecodable NIR ({e})", f.name))?);
        std::fs::write(&path, f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    mains.sort();
    Ok(mains)
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_default()
}
