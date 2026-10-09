//! The cost of a wrapper object per completion (#70): 1000 completions of each input, then an
//! evaluation. Run with `cargo test --release -p repl --test completion_cost -- --ignored
//! --nocapture`.

#[path = "../../scalai/src/libs.rs"]
mod libs;

use repl::Session;
use std::time::Instant;
use vm::{Classpath, Vm};

fn measure() -> Result<(), String> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let mut session = Session::new(vm, image, 80).map_err(|e| e.to_string())?;
    session.eval("val x = List(1, 2)");
    for input in ["x.hea", "x.", "x.toString."] {
        let start = Instant::now();
        for _ in 0..1000 {
            session.complete(input, input.len());
        }
        eprintln!("{input:?}: {:?} per completion", start.elapsed() / 1000);
    }
    let start = Instant::now();
    session.eval("val y = x.head");
    eprintln!("an evaluation after them: {:?}", start.elapsed());
    session.shutdown().map_err(|e| e.to_string())
}

#[test]
#[ignore = "a measurement"]
fn cost_of_a_wrapper_per_completion() {
    std::thread::Builder::new().stack_size(1 << 30).spawn(measure).unwrap().join().unwrap().unwrap();
}
