//! Runs every program in `tests/programs/<name>/` (its NIR, entry point `<name>`) and compares
//! the output with `<name>.check`, which the Scala Native build of the same program printed;
//! cold and from a heap snapshot.

use std::path::{Path, PathBuf};
use std::process::Command;

fn command(dir: &Path, main: &str, snapshot: Option<&Path>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_scivolo"));
    command.args(["--cp", dir.to_str().unwrap(), main]);
    if let Some(file) = snapshot {
        command.env("SCIVOLO_SNAPSHOT", file);
    }
    command
}

fn run(dir: &Path, main: &str, snapshot: Option<&Path>) -> Result<String, String> {
    let out = command(dir, main, snapshot).output().map_err(|e| e.to_string())?;
    match out.status.success() {
        true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        false => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs")
}

fn run_all(snapshot: Option<&Path>) {
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(root()).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_str().unwrap().to_owned();
        let expected = std::fs::read_to_string(dir.join(format!("{name}.check"))).unwrap();
        match run(&dir, &name, snapshot) {
            Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
                eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
                return;
            }
            Ok(out) if out == expected => {}
            Ok(out) => failures.push(format!("{name}: expected\n{expected}\ngot\n{out}")),
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn programs() {
    run_all(None);
}

#[test]
fn programs_from_a_snapshot() {
    let file = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs-snapshot.bin");
    let _ = std::fs::remove_file(&file);
    let hello = root().join("hello");
    if run(&hello, "hello", Some(&file)).is_err_and(|e| e.contains("or the Coursier cache")) {
        return;
    }
    let traced = command(&hello, "hello", Some(&file)).env("SCIVOLO_TRACE", "1").output().unwrap();
    assert!(String::from_utf8_lossy(&traced.stderr).contains("snapshot restored"));
    run_all(Some(&file));
}
