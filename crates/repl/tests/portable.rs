//! The VM's portable mode (ADR 0127), as the browser runs it: a session on the library image's
//! bytes, output, refusals and limits. One session, as the heap serves one VM per process.
#![cfg(feature = "portable")]

#[allow(dead_code)]
mod transcript;

use libimage::Image;
use repl::Session;
use transcript::{MISSING, check};
use vm::{Classpath, Vm};

/// A session over the standard libraries, whose image is read from its file's bytes.
fn session() -> Result<Session, String> {
    let mapped = libs::image(&libs::jars()?).map_err(|e| format!("{MISSING}{e}"))?;
    let file = libs::cache_dir().join(format!("libs-{:016x}.img", mapped.key));
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let image: &'static Image =
        Box::leak(Box::new(Image::from_bytes(Vec::leak(bytes)).map_err(|e| e.to_string())?));
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    Session::new(vm, image, 80).map_err(|e| e.to_string())
}

/// Whether `out` is the lines `before`, then one line ending with `last`.
fn is(out: &[String], before: &[&str], last: &str) -> bool {
    out.len() == before.len() + 1
        && out.iter().zip(before).all(|(o, b)| o == b)
        && out[before.len()].ends_with(last)
}

fn has(out: &[String], text: &str) -> bool {
    out.iter().any(|l| l.contains(text))
}

/// Evaluates `input`; an error showing its output unless `want` accepts that.
fn expect(s: &mut Session, input: &str, want: impl Fn(&[String]) -> bool) -> Result<(), String> {
    let out = s.eval(input);
    match want(&out) {
        true => Ok(()),
        false => Err(format!("{input}\n=> {}", out.join("\n"))),
    }
}

fn run() -> Result<(), String> {
    let s = &mut session()?;
    expect(s, "println(\"a\"); System.err.println(\"b\"); print(\"c\"); 1", |o| {
        is(o, &["a", "b", "c"], ": Int = 1")
    })?;
    expect(s, "new Thread(() => ()).start()", |o| has(o, "threads are not supported here"))?;
    expect(s, "def deep(n: Int): Int = if n == 0 then 0 else 1 + deep(n - 1)", |_| true)?;
    expect(s, "try deep(100_000_000) catch case _: StackOverflowError => -1", |o| is(o, &[], ": Int = -1"))?;
    expect(s, "val o = new Object; o.synchronized(o.wait())", |o| {
        has(o, "deadlock: every thread waits forever")
    })?;
    expect(s, "Thread.sleep(20); System.nanoTime() > 0 && System.currentTimeMillis() > 0", |o| {
        is(o, &[], ": Boolean = true")
    })?;
    expect(s, "Iterator.continually(new Array[Byte](1 << 20)).take(2000).map(_.length.toLong).sum", |o| {
        has(o, "java.lang.OutOfMemoryError")
    })?;
    expect(s, "sys.exit(3)", |_| true)?;
    match s.exit() {
        Some(3) => Ok(()),
        code => Err(format!("sys.exit(3) ended the session with {code:?}")),
    }
}

#[test]
fn portable_mode() {
    check(run);
}
