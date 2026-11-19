//! The VM's portable mode (ADR 0127), as the browser runs it: output, refusals and limits. One
//! session, as the heap serves one VM per process.
#![cfg(feature = "portable")]

#[allow(dead_code)]
mod transcript;

use repl::Session;
use transcript::{check, session};

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
    let s = &mut session(&[])?;
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
