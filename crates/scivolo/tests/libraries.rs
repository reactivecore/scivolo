//! Programs of `tests/aot` using libraries run as scripts, compiled by the own frontend with
//! their `//> using dep` dependencies from Coursier; skipped outside CI when Coursier cannot
//! provide them.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Checks the output of program `name` run as a script; with `as_flags`, its dependencies are
/// passed as `--dep` and its directives blanked.
fn check_script(name: &str, as_flags: bool) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/aot").join(name);
    let program = std::fs::read_to_string(src.join(format!("{name}.scala"))).unwrap();
    let deps: Vec<String> = program
        .lines()
        .filter_map(|l| l.strip_prefix("//> using dep "))
        .map(|d| d.trim().to_owned())
        .collect();
    let code: Vec<&str> =
        program.lines().map(|l| if as_flags && l.starts_with("//>") { "" } else { l }).collect();
    if let Err(e) = libs::jars().and_then(|_| libs::deps::fetch(&deps).map_err(|e| e.to_string()))
        && std::env::var_os("CI").is_none()
    {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("libraries").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.scala"));
    std::fs::write(&file, code.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
    let _ = std::fs::remove_dir_all(dir.join("cache").join("scripts"));
    let flags = deps.iter().filter(|_| as_flags).flat_map(|d| ["--dep", d]);
    let out = Command::new(env!("CARGO_BIN_EXE_scivolo"))
        .env("SCIVOLO_TIMEOUT", "300")
        .env("SCIVOLO_FRONTEND", "strict")
        .env("SCIVOLO_CACHE", dir.join("cache"))
        .args(flags)
        .arg(&file)
        .output()
        .unwrap();
    let expected = std::fs::read_to_string(src.join(format!("{name}.check"))).unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn circe_codec_derives_in_a_script() {
    check_script("circe", true);
}

#[test]
fn ujson_runs_in_a_script() {
    check_script("ujson", false);
}

#[test]
fn upickle_derives_in_a_script() {
    check_script("Lh24upickle", false);
}

#[test]
fn pprint_runs_in_a_script() {
    check_script("pprint", false);
    check_script("Lh24pprint", false);
}

#[test]
fn oslib_runs_in_a_script() {
    check_script("oslib", false);
    check_script("osproc", false);
    check_script("Lh24oslib", false);
}

#[test]
fn fastparse_macros_expand_in_a_script() {
    check_script("fastparse", false);
    check_script("fastparsews", false);
}

#[test]
fn mainargs_macros_expand_in_a_script() {
    check_script("mainargs", false);
}

#[test]
fn sourcecode_macros_expand_in_a_script() {
    check_script("sourcecode", false);
}
