//! The file mode: a whole `.scala` file compiled and run in the instance's one VM.

use libimage::Image;
use repl::Ran;
use std::time::Duration;
use vm::{Classpath, Vm};

/// The name of the file a program is compiled from.
const FILE: &str = "main.scala";

/// What a VM of this instance runs with: the library image, the columns of compiler messages,
/// the heap cap in bytes and the time limit of a run.
pub struct Setup {
    pub image: &'static Image,
    pub width: i32,
    pub cap: usize,
    pub limit: Duration,
}

/// Creates the VM over `setup`'s image, which watches interrupts and stops runs at its limit.
pub fn new_vm(setup: &Setup) -> Result<Vm, String> {
    let classpath = Classpath::with_image(&[], setup.image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    vm.set_heap_cap(setup.cap);
    vm.watch_interrupts().map_err(|e| e.to_string())?;
    vm.limit_runs(Some(setup.limit));
    Ok(vm)
}

/// Compiles `text` and runs its main method in this instance's VM; the exit code and a tab
/// before each line with a compile error, then the lines printed, each ended by a newline.
/// Refuses a program whose compile ran a macro: the macro's effects in the VM would reach it, and
/// the instance has no second VM (ADR 0029).
pub fn run_file(setup: &Setup, text: &str) -> String {
    let ran = match new_vm(setup) {
        Err(e) => failed(e),
        Ok(mut vm) => match repl::compile_program((FILE, text), setup.image, setup.width as usize, &mut vm) {
            Err(ran) => ran,
            Ok(_) if vm.macro_expansions() > 0 => failed(MACROS.to_owned()),
            Ok(program) => repl::run_program(&mut vm, program),
        },
    };
    let mut out = ran.exit.to_string();
    ran.error_lines.iter().for_each(|l| out += &format!("\t{l}"));
    out.push('\n');
    ran.lines.iter().for_each(|l| out += &format!("{l}\n"));
    out
}

/// What a run whose compile ran a macro prints.
const MACROS: &str = "-- unsupported: running a program whose compile ran a macro";

fn failed(why: String) -> Ran {
    Ran { lines: vec![why], exit: 1, error_lines: Vec::new() }
}
