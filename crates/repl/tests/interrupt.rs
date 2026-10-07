//! Ctrl-C while an input runs stops that input and keeps the session.

#[path = "../../scalai/src/libs.rs"]
mod libs;

use repl::Session;
use std::time::{Duration, Instant};
use vm::{Classpath, Vm};

/// What `session` prints for `input` while a SIGINT arrives after 300 ms.
fn interrupted(session: &mut Session, input: &str) -> Vec<String> {
    let signal = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: `raise` has no preconditions; the handler `watch_interrupts` installed runs.
        unsafe { libc::raise(libc::SIGINT) };
    });
    let started = Instant::now();
    let out = session.eval(input);
    signal.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(20), "{input} took {:?}", started.elapsed());
    out
}

fn assert_stopped(out: &[String]) {
    assert!(out.first().is_some_and(|l| l.starts_with("java.lang.ThreadDeath")), "{out:?}");
}

fn run() -> Result<(), String> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let mut session = Session::new(vm, image, 80).map_err(|e| e.to_string())?;
    session.watch_interrupts().map_err(|e| e.to_string())?;

    assert_stopped(&interrupted(&mut session, "while true do ()"));
    assert_eq!(session.eval("1 + 1"), ["val res1: Int = 2"]);

    assert_stopped(&interrupted(&mut session, "Thread.sleep(60000)"));

    session.eval("var done = false");
    session.eval("def spin(): Unit = while true do ()");
    let rethrow = "try spin() catch { case e: Throwable => done = true; throw e }";
    assert_stopped(&interrupted(&mut session, rethrow));
    assert_eq!(session.eval("done"), ["val res4: Boolean = true"]);
    Ok(())
}

#[test]
fn ctrl_c_stops_the_input() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
