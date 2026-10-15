//! `scalai FILE` under each `SCALAI_FRONTEND` mode, with a fake scala-cli that logs its calls
//! and "compiles" every file into the committed NIR of `tests/programs/hello`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOOD: &str = "#!/usr/bin/env scalai\n//> using scala 3.9.0\n@main def good(): Unit = println(\"hi\")\n";
const BAD: &str = "@main def bad(): Unit =\n  val n: Int = \"no\"\n  println(n)\n";
const WARN: &str = "@main def warn(): Unit =\n  val Some(n) = Option(1)\n  println(n)\n";
const FAKE: &str = "#!/bin/sh
echo \"$@\" >> LOG
while [ \"$#\" -gt 0 ]; do
  if [ \"$1\" = -d ]; then mkdir -p \"$2\" && cp NIR/*.nir \"$2\"; fi
  shift
done
";
const HELLO: &str = "Hello, world!\n";

struct Run {
    out: Output,
    stdout: String,
    stderr: String,
}

fn scalai(dir: &Path, file: &str, mode: &str) -> Run {
    scalai_with(dir, &[], file, mode)
}

fn scalai_with(dir: &Path, options: &[&str], file: &str, mode: &str) -> Run {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .args(options.iter().flat_map(|o| ["-O", o]))
        .env("SCALAI_LIBS", root.join("ref/libs"))
        .env("SCALAI_CACHE", dir.join("cache"))
        .env("SCALAI_FRONTEND", mode)
        .env("SCALAI_SCALA_CLI", dir.join("scala-cli"))
        .env("SCALAI_TIMEOUT", "60")
        .arg(dir.join(file))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    Run { out, stdout, stderr }
}

/// How often the fake scala-cli ran.
fn cli_calls(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("log")).map_or(0, |log| log.lines().count())
}

#[test]
fn the_frontend_mode_chooses_the_compiler() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !typer::dump::library_jars(&root.join("ref/libs"))[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scripts");
    let _ = std::fs::remove_dir_all(dir.join("cache/scripts"));
    let _ = std::fs::remove_file(dir.join("log"));
    std::fs::create_dir_all(&dir).unwrap();
    let fatal = format!("//> using options -Werror\n{WARN}");
    let files = [("good.scala", GOOD), ("bad.scala", BAD), ("good.sc", "println(1)\n"), ("warn.scala", WARN)];
    for (file, text) in files.into_iter().chain([("fatal.scala", fatal.as_str())]) {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let nir = root.join("tests/programs/hello").canonicalize().unwrap();
    let fake = FAKE.replace("LOG", dir.join("log").to_str().unwrap()).replace("NIR", nir.to_str().unwrap());
    std::fs::write(dir.join("scala-cli"), fake).unwrap();
    Command::new("chmod").arg("+x").arg(dir.join("scala-cli")).status().unwrap();

    let r = scalai(&dir, "good.scala", "");
    assert!(r.out.status.success(), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), r.stderr.as_str(), cli_calls(&dir)), ("hi\n", "", 0));

    let r = scalai(&dir, "bad.scala", "");
    assert!(r.stderr.ends_with("bad.scala: falling back to scala-cli (type errors)\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), (HELLO, 1));

    let r = scalai(&dir, "bad.scala", "");
    assert!(r.stderr.ends_with("bad.scala: compiled by scala-cli (type errors)\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), (HELLO, 1));

    let r = scalai(&dir, "bad.scala", "strict");
    assert!(!r.out.status.success());
    assert!(r.stderr.contains("1 error found\n"), "{}", r.stderr);
    assert!(r.stderr.ends_with("bad.scala: the own frontend refuses it (type errors)\n"), "{}", r.stderr);
    assert_eq!(cli_calls(&dir), 1);

    let r = scalai(&dir, "good.scala", "jvm");
    assert_eq!((r.stdout.as_str(), r.stderr.as_str(), cli_calls(&dir)), (HELLO, "", 2));
    let r = scalai(&dir, "good.scala", "jvm");
    assert_eq!((r.stdout.as_str(), r.stderr.as_str(), cli_calls(&dir)), (HELLO, "", 2));

    let r = scalai(&dir, "good.sc", "");
    assert!(r.stderr.ends_with("good.sc: falling back to scala-cli (.sc scripts)\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), (HELLO, 3));

    let r = scalai(&dir, "warn.scala", "");
    assert!(r.out.status.success(), "{}", r.stderr);
    assert!(r.stderr.ends_with("1 warning found\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), ("1\n", 3));

    let r = scalai_with(&dir, &["-Werror"], "warn.scala", "");
    assert!(!r.out.status.success());
    assert!(r.stderr.contains("No warnings can be incurred under -Werror"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), ("", 3));

    let r = scalai(&dir, "fatal.scala", "");
    assert!(r.stderr.ends_with("fatal.scala: warnings under -Werror\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), ("", 3));

    let r = scalai_with(&dir, &["-Wfoo"], "good.scala", "");
    assert!(r.stderr.ends_with("good.scala: falling back to scala-cli (option `-Wfoo`)\n"), "{}", r.stderr);
    assert_eq!((r.stdout.as_str(), cli_calls(&dir)), (HELLO, 4));
    assert!(std::fs::read_to_string(dir.join("log")).unwrap().contains("--scalac-option -Wfoo"));

    let r = scalai(&dir, "good.scala", "fast");
    assert!(!r.out.status.success());
    assert!(r.stderr.contains("SCALAI_FRONTEND=fast: expected strict or jvm"), "{}", r.stderr);
}
