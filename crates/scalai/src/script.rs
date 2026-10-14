//! Compiles a `.scala` or `.sc` file to NIR through the own frontend or scala-cli, cached by
//! path, content, frontend mode and versions.

use crate::frontend::{self, Mode, Refused};
use crate::libs::{NATIVE, SCALA, cache_dir};
use crate::mains;
use libimage::{Image, cache};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The file of a cache entry scala-cli compiled after a refusal, holding the reason.
const FALLBACK: &str = "fallback";

/// Whether `arg` names a source file to compile rather than a main class.
pub fn is_source(arg: &str) -> bool {
    arg.ends_with(".scala") || arg.ends_with(".sc")
}

/// The NIR directory of `source` and its main class: `chosen`, else the only one. The own
/// frontend reads the library's TASTy from `image`.
pub fn main_of(
    source: &str,
    chosen: Option<String>,
    image: Option<&'static Image>,
) -> Result<(PathBuf, String), String> {
    let (classes, mains) = compile(source, Mode::from_env()?, image)?;
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
fn compile(file: &str, mode: Mode, image: Option<&'static Image>) -> Result<(PathBuf, Vec<String>), String> {
    let source = Path::new(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
    let text = std::fs::read(&source).map_err(|e| format!("{file}: {e}"))?;
    let mut h = DefaultHasher::new();
    (&source, &text, mode, env!("SCALAI_BUILD"), SCALA, NATIVE).hash(&mut h);
    let scripts = cache_dir().join("scripts");
    let dir = scripts.join(format!("{:016x}", h.finish()));
    cache::touch(&dir);
    if let Some(cached) = cached(&dir) {
        if let Ok(why) = std::fs::read_to_string(dir.join(FALLBACK)) {
            eprintln!("scalai: {file}: compiled by scala-cli ({why})");
        }
        return Ok(cached);
    }
    if cache::prune_due(&scripts) {
        cache::prune(&scripts, cache::UNUSED, cache::is_key);
        cache::prune(&scripts, cache::ABANDONED, |n| cache::is_staging(n, "", "."));
    }
    let staging = dir.with_extension(std::process::id().to_string());
    let built = build(file, &source, &text, mode, image, &staging);
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

/// Compiles `source`, named `file` on the command line and holding `text`, into `out`/`classes`
/// as `mode` chooses, and lists its main classes in `out`/`main`.
fn build(
    file: &str,
    source: &Path,
    text: &[u8],
    mode: Mode,
    image: Option<&'static Image>,
    out: &Path,
) -> Result<(), String> {
    let classes = out.join("classes");
    let mut mains = Vec::new();
    match mode {
        Mode::Jvm => scala_cli(source, out, &mut mains)?,
        _ => match frontend::compile(source, text, image, &classes) {
            Ok(own) => mains = own,
            Err(Refused { why, errors }) if mode == Mode::Strict => {
                errors.iter().for_each(|line| eprintln!("{line}"));
                return Err(format!("{file}: the own frontend refuses it ({why})"));
            }
            Err(Refused { why, .. }) => {
                eprintln!("scalai: {file}: falling back to scala-cli ({why})");
                let _ = std::fs::remove_dir_all(&classes);
                scala_cli(source, out, &mut mains)?;
                std::fs::write(out.join(FALLBACK), why).map_err(|e| format!("{}: {e}", out.display()))?;
            }
        },
    }
    if mains.is_empty() {
        return Err(format!("{file}: no main method"));
    }
    mains.sort();
    std::fs::write(out.join("main"), mains.join("\n")).map_err(|e| format!("{}: {e}", out.display()))
}

/// Compiles `source` into `out`/`classes` through scala-cli and adds its main classes to `mains`.
fn scala_cli(source: &Path, out: &Path, mains: &mut Vec<String>) -> Result<(), String> {
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
    mains::in_dir(&classes, mains)
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
