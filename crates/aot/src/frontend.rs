//! Scala sources to NIR through the own frontend, typed together as one unit, cached by the
//! sources, their options, the library jars and the build.

use crate::R;
use libimage::{Image, cache};
use libs::{BUILD, NATIVE, SCALA, cache_dir};
use lower::directives::{directives, without_header};
use macros::{Settings, VmMacros};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::source::{SourceFile, Sources};

/// Source files read, with the compiler options and dependencies of their `//> using`
/// directives.
pub struct Unit {
    files: Vec<SourceFile>,
    options: Vec<String>,
    deps: Vec<String>,
}

impl Unit {
    /// The dependencies the directives name.
    pub fn deps(&self) -> &[String] {
        &self.deps
    }
}

/// Reads `paths`, each named by its path as given, with the shebang line blanked, or the first
/// directive the own frontend does not honour.
pub fn read(paths: &[PathBuf]) -> R<Unit> {
    let mut unit = Unit { files: Vec::new(), options: Vec::new(), deps: Vec::new() };
    for path in paths {
        let name = path.to_string_lossy().into_owned();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        let lower::directives::Directives { options, deps } =
            directives(&text, SCALA).map_err(|d| format!("{name}: {d} is not supported"))?;
        unit.options.extend(options.into_iter().map(str::to_owned));
        unit.deps.extend(deps.into_iter().map(|(_, d)| d.to_owned()));
        unit.files.push(SourceFile::new(name, without_header(&text)));
    }
    Ok(unit)
}

/// The NIR directory of `unit`, typed against `image` of the library jars `jars`, and its main
/// classes, compiled unless cached. A compile goes to a directory of its own, renamed into
/// place when complete.
pub fn compile(unit: &Unit, jars: &[PathBuf], image: &'static Image) -> R<(PathBuf, Vec<String>)> {
    let units = cache_dir().join("aot-units");
    let dir = units.join(format!("{:016x}", key(unit, jars)));
    cache::touch(&dir);
    if let Some(hit) = cached(&dir) {
        return Ok(hit);
    }
    if cache::prune_due(&units) {
        cache::prune(&units, cache::UNUSED, cache::is_key);
        cache::prune(&units, cache::ABANDONED, |n| cache::is_staging(n, "", "."));
    }
    let staging = dir.with_extension(std::process::id().to_string());
    let built = build(unit, image, &staging.join("classes")).and_then(|mains| {
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

/// Compiles `unit` against `image` into the NIR directory `classes`, uncached.
pub fn compile_into(unit: &Unit, image: &'static Image, classes: &Path) -> R<()> {
    build(unit, image, classes).map(drop)
}

/// The cache key of `unit` compiled against `jars`.
// Temporary until #115 (per-file NIR reuse): one key for all sources, so a change to one
// recompiles all.
fn key(unit: &Unit, jars: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    (BUILD, SCALA, NATIVE, &unit.options).hash(&mut h);
    for f in &unit.files {
        let path = Path::new(&f.name);
        (path.canonicalize().unwrap_or_else(|_| path.to_owned()), &f.name, &f.text).hash(&mut h);
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

/// Writes the NIR files of `unit` under `classes` and returns their main classes, sorted; its
/// macro calls run in a VM over `image`.
fn build(unit: &Unit, image: &'static Image, classes: &Path) -> R<Vec<String>> {
    let mut options: Vec<&str> = unit.options.iter().map(String::as_str).collect();
    let settings = Settings::from_options(&options);
    options.retain(|o| !Settings::reads(o));
    let cfg = Config::checked(&options).map_err(|o| format!("option `{o}`"))?;
    let classpath = vm::Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let mut vm = vm::Vm::new(classpath).map_err(|e| format!("the macro VM: {e}"))?;
    let compiled = catch_unwind(AssertUnwindSafe(|| {
        let sources = unit.files.iter().map(|f| SourceFile::new(f.name.clone(), f.text.clone())).collect();
        let mut macros = VmMacros { vm: &mut vm, sources, settings };
        lower::compile_sources(&mut tasty::Universe::open_image(image), &unit.files, cfg, Some(&mut macros))
    }));
    let (files, warnings) = compiled
        .map_err(|p| lower::output::panicked(p.as_ref()))?
        .map_err(|r| format!("the own frontend refuses the sources: {}", r.what))?;
    if !cfg.warnings.nowarn && !warnings.items.is_empty() {
        let located = Sources::new(&unit.files)?;
        for w in &warnings.items {
            let at = w.offset.map_or_else(String::new, |o| format!("{}: ", lower::position(&located, o)));
            eprintln!("warning: {at}{}", w.message);
        }
        if cfg.warnings.fatal {
            return Err("No warnings can be incurred under -Werror (or -Xfatal-warnings)".into());
        }
    }
    lower::output::write_nir(files, classes)
}
