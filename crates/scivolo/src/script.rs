//! Compiles a `.scala` or `.sc` file to NIR through the own frontend or scala-cli, cached by
//! path, content, frontend mode, versions and the classpath entries' paths, sizes and times.

use crate::frontend::{self, Mode, Refused};
use crate::mains;
use libimage::{Image, cache};
use libs::{BUILD, NATIVE, SCALA, cache_dir};
use lower::directives::without_header;
use macros::{Macros, VmMacros};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use syntax::source::SourceFile;
use vm::Vm;

/// The file of a cache entry scala-cli compiled after a refusal, holding the reason.
const FALLBACK: &str = "fallback";

/// Whether `arg` names a source file to compile rather than a main class.
pub fn is_source(arg: &str) -> bool {
    arg.ends_with(".scala") || arg.ends_with(".sc")
}

/// The NIR directory of `source`, compiled with compiler options `options` against classpath
/// `cp`, and its main class: `chosen`, else the only one. The own frontend reads the library's
/// TASTy from `image` and runs macros in `vm`.
pub fn main_of(
    source: &str,
    chosen: Option<String>,
    (image, cp): (Option<&'static Image>, &[PathBuf]),
    options: &[String],
    vm: &mut Vm,
) -> Result<(PathBuf, String), String> {
    let (classes, mains) = compile(source, Mode::from_env()?, (image, cp), options, vm)?;
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
fn compile(
    file: &str,
    mode: Mode,
    (image, cp): (Option<&'static Image>, &[PathBuf]),
    options: &[String],
    vm: &mut Vm,
) -> Result<(PathBuf, Vec<String>), String> {
    let source = Path::new(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
    let text = std::fs::read(&source).map_err(|e| format!("{file}: {e}"))?;
    let mut h = DefaultHasher::new();
    (&source, &text, mode, options, BUILD, SCALA, NATIVE).hash(&mut h);
    for entry in cp {
        let meta = std::fs::metadata(entry).ok();
        (entry.canonicalize().ok(), meta.as_ref().map(|m| (m.len(), m.modified().ok()))).hash(&mut h);
    }
    let scripts = cache_dir().join("scripts");
    let dir = scripts.join(format!("{:016x}", h.finish()));
    cache::touch(&dir);
    if let Some(cached) = cached(&dir) {
        if let Ok(why) = std::fs::read_to_string(dir.join(FALLBACK)) {
            eprintln!("scivolo: {file}: compiled by scala-cli ({why})");
        }
        return Ok(cached);
    }
    if cache::prune_due(&scripts) {
        cache::prune(&scripts, cache::UNUSED, cache::is_key);
        cache::prune(&scripts, cache::ABANDONED, |n| cache::is_staging(n, "", "."));
    }
    let staging = dir.with_extension(std::process::id().to_string());
    let header_free = without_header(&String::from_utf8_lossy(&text));
    let mut runner = VmMacros { vm, sources: vec![SourceFile::new(source.to_string_lossy(), header_free)] };
    let macros = Some(&mut runner as &mut dyn Macros);
    let built = build(file, (&source, &text, options), mode, (image, macros), &staging);
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

/// Compiles `source`, named `file` on the command line and holding `text`, with compiler
/// options `options` into `out`/`classes` as `mode` chooses, and lists its main classes in
/// `out`/`main`.
fn build(
    file: &str,
    (source, text, options): (&Path, &[u8], &[String]),
    mode: Mode,
    (image, macros): (Option<&'static Image>, Option<&mut dyn Macros>),
    out: &Path,
) -> Result<(), String> {
    let classes = out.join("classes");
    let mut mains = Vec::new();
    match mode {
        Mode::Jvm => scala_cli(source, options, out, &mut mains)?,
        _ => match frontend::compile((source, text), image, &classes, options, macros) {
            Ok(own) => mains = own,
            Err(Refused { why, errors, fatal: true }) => {
                errors.iter().for_each(|line| eprintln!("{line}"));
                return Err(format!("{file}: {why}"));
            }
            Err(Refused { why, errors, .. }) if mode == Mode::Strict => {
                errors.iter().for_each(|line| eprintln!("{line}"));
                return Err(format!("{file}: the own frontend refuses it ({why})"));
            }
            Err(Refused { why, .. }) => {
                eprintln!("scivolo: {file}: falling back to scala-cli ({why})");
                let _ = std::fs::remove_dir_all(&classes);
                scala_cli(source, options, out, &mut mains)?;
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

/// Compiles `source` with compiler options `options` into `out`/`classes` through scala-cli and
/// adds its main classes to `mains`; reported errors fail it also when scala-cli exits with 0.
fn scala_cli(source: &Path, options: &[String], out: &Path, mains: &mut Vec<String>) -> Result<(), String> {
    let cli = std::env::var_os("SCIVOLO_SCALA_CLI").unwrap_or_else(|| "scala-cli".into());
    let classes = out.join("classes");
    let run = Command::new(&cli)
        .args(["--power", "compile", "--quiet", "--native", "--native-version", NATIVE, "--scala", SCALA])
        .args(["--scalac-option", "-Wconf:msg=reserved for internal compiler use:s"])
        .args(options.iter().flat_map(|o| ["--scalac-option", o.as_str()]))
        .arg("--workspace")
        .arg(out.join("work"))
        .arg("-d")
        .arg(&classes)
        .arg(source)
        .stdout(Stdio::inherit())
        .output()
        .map_err(|e| format!("{}: {e}", cli.to_string_lossy()))?;
    let stderr = String::from_utf8_lossy(&run.stderr);
    eprint!("{stderr}");
    let errors = stderr.lines().any(|l| l.ends_with(" error found") || l.ends_with(" errors found"));
    if !run.status.success() || errors {
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
