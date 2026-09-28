//! Compiles a `.scala` or `.sc` file to NIR through scala-cli, cached by its content.

use crate::libs::{NATIVE, SCALA, cache_dir};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The mangled signature of `main(Array[String]): Unit` on a main class.
const MAIN: &str = "D4mainLAL16java.lang.String_uEo";

/// Whether `arg` names a source file to compile rather than a main class.
pub fn is_source(arg: &str) -> bool {
    (arg.ends_with(".scala") || arg.ends_with(".sc")) && Path::new(arg).is_file()
}

/// The NIR directory and main classes of `source`, compiled first unless cached.
pub fn compile(source: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let source = source.canonicalize().map_err(|e| format!("{}: {e}", source.display()))?;
    let text = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut h = DefaultHasher::new();
    (&source, &text, env!("CARGO_PKG_VERSION"), SCALA, NATIVE).hash(&mut h);
    let dir = cache_dir().join("scripts").join(format!("{:016x}", h.finish()));
    let (classes, main_file) = (dir.join("classes"), dir.join("main"));
    if let Ok(mains) = std::fs::read_to_string(&main_file) {
        return Ok((classes, mains.lines().map(str::to_owned).collect()));
    }
    let cli = std::env::var_os("SCALAI_SCALA_CLI").unwrap_or_else(|| "scala-cli".into());
    let status = Command::new(&cli)
        .args(["--power", "compile", "--quiet", "--native", "--native-version", NATIVE, "--scala", SCALA])
        .args(["--scalac-option", "-Wconf:msg=reserved for internal compiler use:s"])
        .arg("--workspace")
        .arg(dir.join("work"))
        .arg("-d")
        .arg(&classes)
        .arg(&source)
        .status()
        .map_err(|e| format!("{}: {e}", cli.to_string_lossy()))?;
    if !status.success() {
        return Err(format!("compiling {} failed", source.display()));
    }
    let mains = main_classes(&classes)?;
    std::fs::write(&main_file, mains.join("\n")).map_err(|e| format!("{}: {e}", main_file.display()))?;
    Ok((classes, mains))
}

/// `chosen` if given, else the only one of `mains`.
pub fn pick_main(source: &str, mains: &[String], chosen: Option<String>) -> Result<String, String> {
    match (chosen, mains) {
        (Some(c), _) if mains.contains(&c) => Ok(c),
        (Some(c), _) => Err(format!("{source}: no main class {c}; it has {}", mains.join(", "))),
        (None, [one]) => Ok(one.clone()),
        (None, []) => Err(format!("{source}: no main method")),
        (None, _) => {
            Err(format!("{source}: several main classes, choose one with --main: {}", mains.join(", ")))
        }
    }
}

/// The classes in `classes` that define `main`, sorted.
fn main_classes(classes: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(classes).map_err(|e| format!("{}: {e}", classes.display()))?;
    let mut mains = Vec::new();
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "nir")) {
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        mains.extend(defines_main(&data).map_err(|e| format!("{}: {e}", path.display()))?);
    }
    mains.sort();
    Ok(mains)
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
    use super::pick_main;

    #[test]
    fn picks_the_only_or_the_chosen_main() {
        let mains = ["one".to_owned(), "two".to_owned()];
        assert_eq!(pick_main("f", &mains[..1], None), Ok("one".to_owned()));
        assert_eq!(pick_main("f", &mains, Some("two".to_owned())), Ok("two".to_owned()));
        assert!(pick_main("f", &mains, None).unwrap_err().contains("--main"));
        assert!(pick_main("f", &mains, Some("three".to_owned())).is_err());
        assert!(pick_main("f", &[], None).is_err());
    }
}
