//! The Java sources of a run: read and parsed for their declarations, then compiled by javac
//! against the run's class files, as sbt and scala-cli do after dotty.

use std::path::{Path, PathBuf};
use std::process::Command;
use tasty::JavaSources;

/// The extension of a Java source.
const JAVA: &str = ".java";

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

/// Compiles Java sources `files` with javac into directory `out`, against `out` and the jars
/// `classpath`; the refusal of an output jar, or of a missing or failing javac.
pub fn compile(files: &[&String], out: &Path, classpath: &[PathBuf]) -> Result<(), String> {
    if out.extension().is_some_and(|e| e == "jar") {
        return Err("Java sources need an output directory, not a jar".to_owned());
    }
    let classpath = std::iter::once(out.to_path_buf()).chain(classpath.iter().cloned());
    let classpath = std::env::join_paths(classpath).map_err(|e| format!("the classpath: {e}"))?;
    let mut javac = Command::new("javac");
    javac.args(["-encoding", "UTF-8", "-d"]).arg(out).arg("-cp").arg(classpath).args(files);
    let status = javac.status().map_err(|e| format!("Java sources need javac on the PATH: {e}"))?;
    if !status.success() {
        return Err(format!("javac failed: {status}"));
    }
    Ok(())
}
