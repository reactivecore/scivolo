//! The VM's portable mode (ADR 0127), as the browser runs it: a session on the library image's
//! bytes, output, refusals and limits. One session, as the heap serves one VM per process.
#![cfg(feature = "portable")]

#[allow(dead_code)]
mod transcript;

use libimage::Image;
use repl::Session;
use std::time::{Duration, Instant};
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
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    vm.set_heap_cap(HEAP_CAP);
    Session::new(vm, image, 80).map_err(|e| e.to_string())
}

/// Bytes of live objects the session may hold, small so that garbage must be collected.
const HEAP_CAP: usize = 48 << 20;

/// Inputs of 256 KiB of garbage each, together five times `HEAP_CAP`.
const INPUTS: usize = 1000;

/// Inputs after which the session forgets its history, which every input types against.
const RESET_EVERY: usize = 100;

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
    expect(s, ":load x.scala", |o| has(o, "files are not supported here"))?;
    time_limit(s)?;
    collections(s)?;
    expect(s, "sys.exit(3)", |_| true)?;
    match s.exit() {
        Some(3) => Ok(()),
        code => Err(format!("sys.exit(3) ended the session with {code:?}")),
    }
}

/// The time limit stops a loop and a sleep as Ctrl-C does, and the session keeps its definitions.
fn time_limit(s: &mut Session) -> Result<(), String> {
    s.watch_interrupts().map_err(|e| e.to_string())?;
    s.limit_runs(Some(Duration::from_millis(300)));
    let stopped = |o: &[String]| o.first().is_some_and(|l| l.starts_with("java.lang.ThreadDeath"));
    let started = Instant::now();
    expect(s, "val kept = 41", |_| true)?;
    expect(s, "while true do ()", stopped)?;
    expect(s, "Thread.sleep(60000)", stopped)?;
    if started.elapsed() > Duration::from_secs(20) {
        return Err(format!("the time limit stopped the inputs after {:?}", started.elapsed()));
    }
    s.limit_runs(None);
    expect(s, "kept + 1", |o| is(o, &[], ": Int = 42"))
}

/// Garbage of many inputs and of one long loop stays under `HEAP_CAP`, live values survive the
/// collections intact, and live data past the cap throws `OutOfMemoryError` without ending the
/// session.
fn collections(s: &mut Session) -> Result<(), String> {
    for i in 0..INPUTS {
        if i % RESET_EVERY == 0 {
            s.reset();
        }
        expect(s, "Array.fill(64)(new Array[Byte](4096)).length", |o| is(o, &[], ": Int = 64"))?;
    }
    expect(s, "val keep = Vector.tabulate(20000)(i => s\"v$i\")", |_| true)?;
    expect(
        s,
        "val table = scala.collection.mutable.HashMap.from((0 until 5000).map(i => i -> i.toString))",
        |_| true,
    )?;
    let lap = "var n = 0L; var i = 0; while i < 4000 do { n += new Array[Byte](100000).length; i += 1 }; n";
    expect(s, lap, |o| o.last().is_some_and(|l| l.ends_with(": Long = 400000000L")))?;
    let each = "(1 to 3000).foldLeft(0L)((n, _) => n + Array.fill(10)(new Array[Long](1000)).length)";
    expect(s, each, |o| is(o, &[], ": Long = 30000L"))?;
    expect(s, "keep(12345) + keep.map(_.length).sum + table(4321)", |o| {
        is(o, &[], "= \"v123451088904321\"")
    })?;
    expect(s, "val big = Array.fill(200)(new Array[Byte](1 << 20)); big.length", |o| {
        has(o, "java.lang.OutOfMemoryError")
    })?;
    expect(s, "keep.last + table.size", |o| is(o, &[], ": String = \"v199995000\""))
}

#[test]
fn portable_mode() {
    check(run);
}
