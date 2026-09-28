//! Compiles a `.scala` or `.sc` file to NIR through scala-cli, cached by path, content and
//! versions.

use crate::libs::{NATIVE, SCALA, cache_dir};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The mangled signature of the static `main(Array[String]): Unit` of a main class.
const MAIN: &str = "D4mainLAL16java.lang.String_uEo";

/// Whether `arg` names a source file to compile rather than a main class.
pub fn is_source(arg: &str) -> bool {
    arg.ends_with(".scala") || arg.ends_with(".sc")
}

/// The NIR directory of `source` and its main class: `chosen`, else the only one.
pub fn main_of(source: &str, chosen: Option<String>) -> Result<(PathBuf, String), String> {
    let (classes, mains) = compile(Path::new(source))?;
    Ok((classes, pick(source, &mains, chosen)?))
}

fn pick(source: &str, mains: &[String], chosen: Option<String>) -> Result<String, String> {
    let list = mains.join(", ");
    match chosen {
        Some(c) if mains.contains(&c) => Ok(c),
        Some(c) => Err(format!("{source}: no main class {c}; it has {list}")),
        None if mains.len() == 1 => Ok(mains[0].clone()),
        None => Err(format!("{source}: several main classes, choose one with --main: {list}")),
    }
}

/// The NIR directory and main classes of `source`, compiled first unless cached. A compile
/// goes to a directory of its own, renamed into place when complete.
fn compile(source: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let source = source.canonicalize().map_err(|e| format!("{}: {e}", source.display()))?;
    let text = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut h = DefaultHasher::new();
    (&source, &text, env!("CARGO_PKG_VERSION"), SCALA, NATIVE).hash(&mut h);
    let scripts = cache_dir().join("scripts");
    let dir = scripts.join(format!("{:016x}", h.finish()));
    if let Some(cached) = cached(&dir) {
        vm::cache::touch(&dir);
        return Ok(cached);
    }
    vm::cache::prune(&scripts, vm::cache::UNUSED, |n| !n.contains('.'));
    vm::cache::prune(&scripts, vm::cache::ABANDONED, |n| n.contains('.'));
    let staging = dir.with_extension(std::process::id().to_string());
    let built = build(&source, &staging);
    if built.is_ok() && std::fs::rename(&staging, &dir).is_ok() {
        return cached(&dir).ok_or_else(|| format!("{}: unreadable", dir.display()));
    }
    let _ = std::fs::remove_dir_all(&staging);
    built?;
    cached(&dir).ok_or_else(|| format!("{}: incomplete; remove it", dir.display()))
}

/// The classes directory and main classes of a complete cache entry.
fn cached(dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    let mains = std::fs::read_to_string(dir.join("main")).ok()?;
    Some((dir.join("classes"), mains.lines().map(str::to_owned).collect()))
}

/// Compiles `source` into `out`/`classes` and lists its main classes in `out`/`main`.
fn build(source: &Path, out: &Path) -> Result<(), String> {
    let cli = std::env::var_os("SCALAI_SCALA_CLI").unwrap_or_else(|| "scala-cli".into());
    let classes = out.join("classes");
    let status = Command::new(&cli)
        .args(["--power", "compile", "--quiet", "--native", "--native-version", NATIVE, "--scala", SCALA])
        .args(["--scalac-option", "-Wconf:msg=reserved for internal compiler use:s"])
        .arg("--workspace")
        .arg(out.join("work"))
        .arg("-d")
        .arg(&classes)
        .arg(source)
        .status()
        .map_err(|e| format!("{}: {e}", cli.to_string_lossy()))?;
    if !status.success() {
        return Err(format!("compiling {} failed", source.display()));
    }
    let _ = std::fs::remove_dir_all(out.join("work"));
    let mut mains = Vec::new();
    main_classes(&classes, &mut mains)?;
    if mains.is_empty() {
        return Err(format!("{}: no main method", source.display()));
    }
    mains.sort();
    std::fs::write(out.join("main"), mains.join("\n")).map_err(|e| format!("{}: {e}", out.display()))
}

/// Adds the classes under `dir` that define `main` to `mains`.
fn main_classes(dir: &Path, mains: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            main_classes(&path, mains)?;
        } else if path.extension().is_some_and(|e| e == "nir") {
            let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            mains.extend(defines_main(&data).map_err(|e| format!("{}: {e}", path.display()))?);
        }
    }
    Ok(())
}

/// The class in one NIR file that defines `main`, if any.
fn defines_main(data: &[u8]) -> nir::Result<Option<String>> {
    let (mut pool, mut caches) = (nir::pool::Pool::default(), nir::Caches::default());
    let index = nir::Reader::new(data, &mut caches, &mut pool)?.index()?;
    Ok(index.iter().find_map(|&(g, _)| match *pool.global(g) {
        nir::Global::Member(top, sig) if pool.str(sig) == MAIN => match *pool.global(top) {
            nir::Global::Top(name) => Some(pool.str(name).to_owned()),
            _ => None,
        },
        _ => None,
    }))
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn picks_the_only_or_the_chosen_main() {
        let mains = ["one".to_owned(), "two".to_owned()];
        assert_eq!(pick("f", &mains[..1], None), Ok("one".to_owned()));
        assert_eq!(pick("f", &mains, Some("two".to_owned())), Ok("two".to_owned()));
        assert!(pick("f", &mains, None).unwrap_err().contains("--main"));
        assert!(pick("f", &mains, Some("three".to_owned())).is_err());
    }
}
