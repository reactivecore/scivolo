//! `scalai` without a target: the REPL reading inputs from piped stdin.

#[allow(dead_code)]
#[path = "../src/libs.rs"]
mod libs;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const INPUT: &str = "1 + 1
def twice(x: Int): Int =
  x * 2

twice(21)
val s = \"a\" + 1
:quit
println(\"not reached\")
";

const OUTPUT: &str = "val res0: Int = 2
def twice(x: Int): Int
val res1: Int = 42
val s: String = \"a1\"
";

/// The exit code, stdout, stderr and wall time of `scalai` reading `input`, with environment
/// `env` added; `scalai` stops with exit code 124 after 300 s.
fn repl_with(input: &[u8], env: &[(&str, &str)]) -> (Option<i32>, String, String, f64) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .env("SCALAI_TIMEOUT", "300")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (out.status.code(), text(out.stdout), text(out.stderr), start.elapsed().as_secs_f64())
}

/// The exit code, stdout and wall time of `scalai` reading `input`.
fn repl(input: &str) -> (Option<i32>, String, f64) {
    let (code, out, _, secs) = repl_with(input.as_bytes(), &[]);
    (code, out, secs)
}

fn libs_missing() -> bool {
    let missing = libs::jars().is_err() && std::env::var_os("CI").is_none();
    if missing {
        eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
    }
    missing
}

#[test]
fn piped_inputs() {
    if libs_missing() {
        return;
    }
    let (_, _, warm) = repl("");
    let (_, first, startup) = repl("1 + 1\n");
    assert_eq!(first, "val res0: Int = 2\n");
    eprintln!("scalai REPL: {warm:.3} s empty (warm-up), {startup:.3} s to the result of `1 + 1`");
    let (code, out, _) = repl(INPUT);
    assert_eq!(out, OUTPUT);
    assert_eq!(code, Some(0));
    let (code, out, _) = repl("val a = 1\nSystem.exit(3)\nprintln(\"not reached\")\n");
    assert_eq!(out, "val a: Int = 1\n");
    assert_eq!(code, Some(3));
}

#[test]
fn piped_line_endings() {
    if libs_missing() {
        return;
    }
    let (code, out, _) = repl("def inc(x: Int): Int =\r\n  x + 1\r\ninc(1)\r\nval b = (1 +");
    let mut lines = out.lines();
    assert_eq!(lines.next(), Some("def inc(x: Int): Int"));
    assert_eq!(lines.next(), Some("val res0: Int = 2"));
    assert!(lines.next().is_some_and(|l| l.contains("Syntax Error")), "{out}");
    assert_eq!(code, Some(0));
    let (code, out, _, _) = repl_with(b"val s = \"\xff\"\ns.length\n", &[]);
    assert_eq!(out, "val s: String = \"\u{fffd}\"\nval res0: Int = 1\n");
    assert_eq!(code, Some(0));
}

#[test]
fn load_paths_missing_or_unreadable() {
    if libs_missing() {
        return;
    }
    let cwd = std::fs::canonicalize(".").unwrap();
    let (code, out, _) = repl(":load nope.scala\n:load nope/../src\n:load ./src\n:load\n:load a b \n");
    let expected = [
        format!("Couldn't find file \"{}\"", cwd.join("nope.scala").display()),
        format!("Couldn't find file \"{}\"", cwd.join("src").display()),
        format!("Couldn't read file \"{}\": Is a directory (os error 21)", cwd.join("src").display()),
        format!("Couldn't find file \"{}\"", cwd.display()),
        format!("Couldn't find file \"{}\"", cwd.join("a b ").display()),
    ];
    assert_eq!(out.lines().collect::<Vec<_>>(), expected);
    assert_eq!(code, Some(0));
}

#[test]
fn load_nested_or_not_utf8() {
    if libs_missing() {
        return;
    }
    let bad = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("not-utf8.scala");
    std::fs::write(&bad, b"val s = \"\xff\"\n").unwrap();
    let (code, out, _) = repl(&format!(":load tests/load/itself.scala\n:load {}\n1\n", bad.display()));
    let itself = std::fs::canonicalize("tests/load/itself.scala").unwrap();
    let bad = std::fs::canonicalize(&bad).unwrap();
    let expected = [
        format!("Couldn't load file \"{}\": more than 16 nested :load commands", itself.display()),
        format!("Couldn't read file \"{}\": stream did not contain valid UTF-8", bad.display()),
        "val res0: Int = 1".to_owned(),
    ];
    assert_eq!(out.lines().collect::<Vec<_>>(), expected);
    assert_eq!(code, Some(0));
}

#[test]
fn printed_output_comes_first_also_when_silent() {
    if libs_missing() {
        return;
    }
    let input = ":silent\nprintln(\"p1\")\nval x = { println(\"p2\"); 1 }\n:load tests/load/prints.scala\n\
                 :silent\nprintln(\"p3\")\n1\n";
    let (code, out, _) = repl(input);
    assert_eq!(out, "p1\np2\nloading\np3\nval res0: Int = 1\n");
    assert_eq!(code, Some(0));
}

const SPIN: &str = "def spin(): Unit = while true do ()
{ println(\"looping\"); while true do try spin() catch { case _: ThreadDeath => println(\"caught\") } }
";

/// The exit code and stdout of `scalai` reading `input` and, once it printed `looping`, getting
/// `signals` SIGINTs 300 ms apart and reading `rest`; `scalai` stops with exit code 124 after
/// 60 s, and a line other than a definition before `looping` fails at once.
fn interrupted(input: &str, signals: usize, rest: &str) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_scalai"))
        .env("SCALAI_TIMEOUT", "60")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut out = String::new();
    while !out.ends_with("looping\n") {
        let start = out.len();
        let read = stdout.read_line(&mut out).unwrap();
        if read == 0 || !(out[start..].starts_with("def ") || out.ends_with("looping\n")) {
            let _ = child.kill();
            panic!("{out}");
        }
    }
    for _ in 0..signals {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: `kill` has no memory preconditions; the child has not been waited for.
        assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) }, 0);
    }
    let _ = stdin.write_all(rest.as_bytes());
    drop(stdin);
    stdout.read_to_string(&mut out).unwrap();
    (child.wait().unwrap().code(), out)
}

#[test]
fn ctrl_c_stops_the_input() {
    if libs_missing() {
        return;
    }
    let (code, out) = interrupted("println(\"looping\"); while true do ()\n", 1, "1 + 1\n");
    let mut lines = out.lines();
    assert_eq!(lines.next(), Some("looping"));
    assert!(lines.next().is_some_and(|l| l.starts_with("java.lang.ThreadDeath")), "{out}");
    assert!(lines.last().is_some_and(|l| l.ends_with(": Int = 2")), "{out}");
    assert_eq!(code, Some(0));
    let (code, out) = interrupted(SPIN, 2, "1 + 1\n");
    assert_eq!(out, "def spin(): Unit\nlooping\ncaught\n");
    assert_eq!(code, Some(130));
}

#[test]
fn missing_libraries() {
    let empty = env!("CARGO_TARGET_TMPDIR");
    let env = [("SCALAI_LIBS", empty), ("COURSIER_CACHE", empty), ("HOME", empty)];
    let (code, out, err, _) = repl_with(b"1 + 1\n", &env);
    assert_eq!(code, Some(1));
    assert_eq!(out, "");
    assert!(err.starts_with("scalai: auxlib"), "{err}");
}
