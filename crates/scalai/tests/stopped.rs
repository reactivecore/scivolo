//! A main thread stopped by SIGINT inside `synchronized` releases the monitor when it ends.

#[path = "../src/libs.rs"]
mod libs;

use std::path::Path;
use std::time::Duration;
use vm::{Classpath, Vm};

fn run() -> Result<(), String> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs/stopped");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "nir") {
            let top = path.file_stem().unwrap().to_str().unwrap().to_owned();
            vm.add_nir(&top, std::fs::read(&path).unwrap()).map_err(|e| e.to_string())?;
        }
    }
    vm.start_runtime("stopped", &[]).map_err(|e| e.to_string())?;
    vm.watch_interrupts().map_err(|e| e.to_string())?;
    let signal = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: `raise` has no preconditions; the handler `watch_interrupts` installed runs.
        unsafe { libc::raise(libc::SIGINT) };
    });
    let stopped = vm.call_string("Locked$", "spin").unwrap_err().to_string();
    signal.join().unwrap();
    assert!(stopped.contains("java.lang.ThreadDeath"), "{stopped}");
    vm.clear_interrupts();
    assert_eq!(vm.call_string("Locked$", "take").map_err(|e| e.to_string())?, "taken");
    Ok(())
}

#[test]
fn stopped_thread_releases_its_monitors() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
