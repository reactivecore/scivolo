//! Runs every program in `tests/programs/<name>/` (its NIR, entry point `<name>`) and compares
//! the output with `<name>.check`, which the Scala Native build of the same program printed.

use std::path::Path;
use std::process::Command;

fn run(dir: &Path, main: &str) -> Result<String, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .args(["--cp", dir.to_str().unwrap(), main])
        .output()
        .map_err(|e| e.to_string())?;
    match out.status.success() {
        true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        false => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    }
}

#[test]
fn programs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs");
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_str().unwrap().to_owned();
        let expected = std::fs::read_to_string(dir.join(format!("{name}.check"))).unwrap();
        match run(&dir, &name) {
            Err(e) if e.contains("or the Coursier cache") => {
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
