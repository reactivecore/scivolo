//! The Java sources of a run: read and parsed for their declarations, then compiled by javac
//! against the run's class files, as sbt and scala-cli do after dotty.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use tasty::JavaSources;

/// The extension of a Java source.
const JAVA: &str = ".java";

/// The extension of the directory javac writes to when the output is a jar.
const JAVAC_DIR: &str = "javac";

/// Determines whether file `f` is a Java source.
pub fn is_java(f: &str) -> bool {
    f.ends_with(JAVA)
}

/// Reads and parses Java sources `files`; the refusal of one the parser does not read.
pub fn parse(files: &[&String]) -> Result<JavaSources, String> {
    let texts = files.iter().map(|f| {
        let text = std::fs::read_to_string(f).map_err(|e| format!("reading {f}: {e}"))?;
        Ok(((*f).clone(), text))
    });
    JavaSources::parse(&texts.collect::<Result<Vec<_>, String>>()?)
}

/// Compiles Java sources `files` with javac against the output `out` and the jars `classpath`:
/// into `out` if it is a directory; for a jar into a directory beside it, whose class files are
/// returned (path from the root and bytes) and the directory removed. Without javac on the PATH
/// nothing is compiled, with a warning, as dotty only reads Java sources. The refusal of a
/// failing javac.
pub fn compile(
    files: &[&String],
    out: &Path,
    classpath: &[PathBuf],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let jar = out.extension().is_some_and(|e| e == "jar");
    let dir = if jar { out.with_extension(JAVAC_DIR) } else { out.to_path_buf() };
    let classpath = std::iter::once(out.to_path_buf()).chain(classpath.iter().cloned());
    let classpath = std::env::join_paths(classpath).map_err(|e| format!("the classpath: {e}"))?;
    let mut javac = Command::new("javac");
    javac.args(["-encoding", "UTF-8", "-d"]).arg(&dir).arg("-cp").arg(classpath).args(files);
    let status = match javac.status() {
        Ok(status) => status,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!("warning: no javac on the PATH: the Java sources are not compiled");
            return Ok(Vec::new());
        }
        Err(e) => return Err(format!("running javac: {e}")),
    };
    if !status.success() {
        return Err(format!("javac failed: {status}"));
    }
    if !jar {
        return Ok(Vec::new());
    }
    let mut classes = Vec::new();
    class_files(&dir, &dir, &mut classes)?;
    std::fs::remove_dir_all(&dir).map_err(|e| format!("removing {dir:?}: {e}"))?;
    Ok(classes)
}

/// Adds the class files under directory `dir` to `out`, each its path from `root` and its bytes.
fn class_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("reading {dir:?}: {e}"))?
        .map(|e| e.map(|e| e.path()).map_err(|e| format!("reading {dir:?}: {e}")))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            class_files(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "class") {
            let name =
                path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
            out.push((name, std::fs::read(&path).map_err(|e| format!("reading {path:?}: {e}"))?));
        }
    }
    Ok(())
}
