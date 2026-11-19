//! Ctrl-C while an input runs stops that input and keeps the session.

use repl::Session;
use std::sync::mpsc;
use std::time::Duration;
use vm::{Classpath, Vm};

/// How long an interrupted input may take before the test ends the process as failed.
const HARD_LIMIT: Duration = Duration::from_secs(60);

/// What `session` prints for `input` while a SIGINT arrives after 300 ms; ends the process as
/// failed if the input runs past `HARD_LIMIT`.
fn interrupted(session: &mut Session, input: &str) -> Vec<String> {
    let (done, ended) = mpsc::channel::<()>();
    let what = input.to_owned();
    let signal = std::thread::spawn(move || {
        if ended.recv_timeout(Duration::from_millis(300)) != Err(mpsc::RecvTimeoutError::Timeout) {
            return;
        }
        // SAFETY: `raise` has no preconditions; the handler `watch_interrupts` installed runs.
        unsafe { libc::raise(libc::SIGINT) };
        if ended.recv_timeout(HARD_LIMIT) == Err(mpsc::RecvTimeoutError::Timeout) {
            eprintln!("Ctrl-C did not stop `{what}` within {HARD_LIMIT:?}");
            std::process::exit(1);
        }
    });
    let out = session.eval(input);
    drop(done);
    signal.join().unwrap();
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
