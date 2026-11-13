//! The programs `tests/programs/lowered.txt` lists, run as `scivolo FILE.scala` through the Rust
//! frontend and lowering (`SCIVOLO_FRONTEND=strict`); the output must equal `<name>.check`
//! (research 10 §5, level 3).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    let programs: Vec<&str> = programs.lines().filter(|p| !p.is_empty()).collect();
    in_parallel(&programs, |&program| {
        let dir = root.join("tests/programs").join(program);
        let run = Command::new(env!("CARGO_BIN_EXE_scivolo"))
            .env("SCIVOLO_LIBS", &libs)
            .env("SCIVOLO_CACHE", &cache)
            .env("SCIVOLO_FRONTEND", "strict")
            .env("SCIVOLO_SCALA_CLI", "no-scala-cli")
            .env("SCIVOLO_TIMEOUT", "60")
            .args(["--main", program])
            .arg(dir.join(format!("{program}.scala")))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{program}: {stderr}");
        let expected = std::fs::read_to_string(dir.join(format!("{program}.check"))).unwrap();
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{program}");
    });
}

/// Runs `check` on each of `items`, on as many threads as there are cores.
fn in_parallel<T: Sync>(items: &[T], check: impl Fn(&T) + Sync) {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                while let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) {
                    check(item);
                }
            });
        }
    });
}
