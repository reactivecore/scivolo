//! Macro calls of the library `tests/macros/lib` expanded in the REPL and in a script, their
//! implementations run in the VM.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const INPUT: &str = "import macrolib.Macros.*
twice(21)
greet(\"Bob\")
pair(\"a\")
listOf(1, 2)
incremented(41)
plusOneTwice(2)
wrapped(1L)
logged(\"x\")
answer
fail
";

const REPL_OUTPUT: &str = "val res0: Int = 42
val res1: String = \"Hello, Bob!\"
val res2: (String, String) = (\"a\", \"a\")
val res3: List[Int] = List(1, 2)
val res4: Int = 42
val res5: Int = 6
val res6: List[Long] = List(1L)
-- unsupported: definitions from library inline bodies
-- unsupported: quotes.reflect
-- Error: ----------------------------------------------------------------------
1 |fail
  |^^^^
  |Failed to evaluate macro.
  |  Caused by class java.lang.RuntimeException: the macro failed
1 error found
";

const SCRIPT: &str = "import macrolib.Macros.*
@main def run(): Unit =
  println(twice(21))
  println(greet(\"Bob\"))
  println(pair(1.5))
  println(listOf('a', 'b'))
  println(incremented(1))
  val n = 4
  println(plusOneTwice(n))
  println(wrapped(\"w\"))
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scalai(args: &[&str], input: &str, env: &[(&str, &str)]) -> Output {
    let jar = root().join("tests/macros/lib/macros.jar");
    let mut child = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .env("SCALAI_TIMEOUT", "300")
        .envs(env.iter().copied())
        .arg("--cp")
        .arg(jar)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn libs_missing() -> bool {
    let missing = libs::jars().is_err() && std::env::var_os("CI").is_none();
    if missing {
        eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
    }
    missing
}

#[test]
fn macros_expand_in_the_repl() {
    if libs_missing() {
        return;
    }
    let out = scalai(&[], INPUT, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, REPL_OUTPUT, "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn macros_expand_in_a_script() {
    if libs_missing() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("macros");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("run.scala");
    std::fs::write(&file, SCRIPT).unwrap();
    let cache = dir.join("cache");
    let _ = std::fs::remove_dir_all(cache.join("scripts"));
    let env = [("SCALAI_FRONTEND", "strict"), ("SCALAI_CACHE", cache.to_str().unwrap())];
    let out = scalai(&[file.to_str().unwrap()], "", &env);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = "42\nHello, Bob!\n(1.5,1.5)\nList(a, b)\n2\n10\nList(w)\n";
    assert_eq!(stdout, expected, "{}", String::from_utf8_lossy(&out.stderr));
}
