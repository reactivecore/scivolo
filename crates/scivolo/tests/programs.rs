//! Runs every program in `tests/programs/<name>/` (its NIR, entry point `<name>`) and compares
//! the output with `<name>.check`, which the Scala Native build of the same program printed;
//! cold and from a heap snapshot.

use std::path::{Path, PathBuf};
use std::process::Command;

fn command(cp: &[&Path], main: &str, snapshot: Option<&Path>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_scivolo"));
    for dir in cp {
        command.arg("--cp").arg(dir);
    }
    command.arg(main);
    match snapshot {
        Some(file) => command.env("SCIVOLO_SNAPSHOT", file),
        None => command.env("SCIVOLO_NO_SNAPSHOT", "1"),
    };
    command
}

fn run(dir: &Path, main: &str, snapshot: Option<&Path>) -> Result<String, String> {
    let out = command(&[dir], main, snapshot).output().map_err(|e| e.to_string())?;
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
    let traced = |cp: &[&Path]| {
        let out = command(cp, "hello", Some(&file)).env("SCIVOLO_TRACE", "1").output().unwrap();
        (String::from_utf8_lossy(&out.stderr).into_owned(), String::from_utf8_lossy(&out.stdout).into_owned())
    };
    assert!(traced(&[&hello]).0.contains("snapshot restored"));
    let shadowing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("shadowing");
    std::fs::create_dir_all(shadowing.join("java/lang")).unwrap();
    std::fs::write(shadowing.join("java/lang/Shadowing.nir"), b"").unwrap();
    let (trace, out) = traced(&[&hello, &shadowing]);
    assert!(trace.contains("snapshot not used") && out == "Hello, world!\n", "{trace}");
    std::fs::remove_file(&file).unwrap();
    let (trace, _) = traced(&[&hello, &shadowing]);
    assert!(trace.contains("no snapshot") && !file.exists(), "{trace}");
    run(&hello, "hello", Some(&file)).unwrap();
    run_all(Some(&file));
}

#[test]
fn corrupt_cache_snapshot_is_replaced() {
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("corrupt-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let hello = root().join("hello");
    let traced = || {
        let mut command = command(&[&hello], "hello", None);
        command.env_remove("SCIVOLO_NO_SNAPSHOT").env("SCIVOLO_CACHE", &cache).env("SCIVOLO_TRACE", "1");
        String::from_utf8_lossy(&command.output().unwrap().stderr).into_owned()
    };
    if traced().contains("or the Coursier cache") {
        return;
    }
    let mut files = std::fs::read_dir(&cache).unwrap().map(|e| e.unwrap().path());
    let snap = files.find(|p| p.file_name().unwrap().to_string_lossy().starts_with("snap-")).unwrap();
    std::fs::write(&snap, b"corrupt").unwrap();
    assert!(traced().contains("snapshot not used"));
    assert!(traced().contains("snapshot restored"));
}
