//! The runtime's initialization leaves a region every word of which a snapshot can restore.

use vm::{Classpath, Vm};

fn run() -> Result<(), String> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    assert!(vm.snapshot_walk().is_err(), "a walk before initialization");
    vm.start_runtime("snapshot", &[]).map_err(|e| e.to_string())?;
    let walk = vm.snapshot_walk()?;
    assert!(walk.objects > 400 && walk.bytes > 60_000, "{walk:?}");
    assert!(vm.snapshot_file()?.len() > walk.bytes);
    vm.eval_module("java.lang.SystemProperties$").map_err(|e| e.to_string())?;
    let denied = vm.snapshot_walk().unwrap_err();
    assert_eq!(denied, "module java.lang.SystemProperties$ depends on the environment");
    Ok(())
}

#[test]
fn initialized_runtime_walks_without_refusal() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
