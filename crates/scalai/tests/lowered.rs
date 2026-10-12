//! The programs `tests/programs/lowered.txt` lists, run as `scalai FILE.scala` through the Rust
//! frontend and lowering (`SCALAI_FRONTEND=strict`); the output must equal `<name>.check`
//! (research 10 §5, level 3).

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn lowered_programs_print_what_scala_native_prints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let libs = root.join("ref/libs");
    if !typer::dump::library_jars(&libs)[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let cache = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lowered-cache");
    let _ = std::fs::remove_dir_all(cache.join("scripts"));
    let programs = std::fs::read_to_string(root.join("tests/programs/lowered.txt")).unwrap();
    for program in programs.lines().filter(|p| !p.is_empty()) {
        let dir = root.join("tests/programs").join(program);
        let run = Command::new(env!("CARGO_BIN_EXE_scalai"))
            .env("SCALAI_LIBS", &libs)
            .env("SCALAI_CACHE", &cache)
            .env("SCALAI_FRONTEND", "strict")
            .env("SCALAI_SCALA_CLI", "no-scala-cli")
            .env("SCALAI_TIMEOUT", "60")
            .args(["--main", program])
            .arg(dir.join(format!("{program}.scala")))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{program}: {stderr}");
        let expected = std::fs::read_to_string(dir.join(format!("{program}.check"))).unwrap();
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{program}");
    }
}
