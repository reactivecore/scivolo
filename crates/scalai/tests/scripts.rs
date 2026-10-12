//! `scalai FILE` under each `SCALAI_FRONTEND` mode, with a scala-cli binary that does not exist.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOOD: &str = "#!/usr/bin/env scalai\n//> using scala 3.9.0\n@main def good(): Unit = println(\"hi\")\n";
const BAD: &str = "@main def bad(): Unit =\n  val n: Int = \"no\"\n  println(n)\n";

fn scalai(dir: &Path, file: &str, mode: &str) -> (Output, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .env("SCALAI_LIBS", root.join("ref/libs"))
        .env("SCALAI_CACHE", dir.join("cache"))
        .env("SCALAI_FRONTEND", mode)
        .env("SCALAI_SCALA_CLI", "no-scala-cli")
        .arg(dir.join(file))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out, stderr)
}

#[test]
fn the_frontend_mode_chooses_the_compiler() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("ref/libs/scala-library-3.9.0.jar").exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scripts");
    let _ = std::fs::remove_dir_all(dir.join("cache/scripts"));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in [("good.scala", GOOD), ("bad.scala", BAD), ("good.sc", "println(1)\n")] {
        std::fs::write(dir.join(file), text).unwrap();
    }

    let (out, stderr) = scalai(&dir, "good.scala", "");
    assert!(out.status.success(), "{stderr}");
    assert_eq!((String::from_utf8_lossy(&out.stdout).as_ref(), stderr.as_str()), ("hi\n", ""));

    let (out, stderr) = scalai(&dir, "bad.scala", "");
    assert!(!out.status.success());
    assert!(stderr.contains("bad.scala: falling back to scala-cli (type errors)\n"), "{stderr}");
    assert!(stderr.contains("no-scala-cli"), "{stderr}");

    let (out, stderr) = scalai(&dir, "bad.scala", "strict");
    assert!(!out.status.success());
    assert!(stderr.ends_with("bad.scala: the own frontend refuses it (type errors)\n"), "{stderr}");

    let (out, stderr) = scalai(&dir, "good.scala", "jvm");
    assert!(!out.status.success());
    assert!(stderr.contains("no-scala-cli") && !stderr.contains("falling back"), "{stderr}");

    let (out, stderr) = scalai(&dir, "good.sc", "");
    assert!(!out.status.success());
    assert!(stderr.contains("good.sc: falling back to scala-cli (.sc scripts)\n"), "{stderr}");

    let (out, stderr) = scalai(&dir, "good.scala", "fast");
    assert!(!out.status.success());
    assert!(stderr.contains("SCALAI_FRONTEND=fast: expected strict or jvm"), "{stderr}");
}
