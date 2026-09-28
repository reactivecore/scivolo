mod libs;
mod script;

use std::path::PathBuf;
use std::process::ExitCode;
use vm::{Classpath, Vm, VmError};

const USAGE: &str =
    "usage: scalai [--cp DIR_OR_JAR]... (MAIN_CLASS | [--main CLASS] FILE.scala | FILE.sc) [ARGS...]";

/// The interpreter thread's stack; only the pages deep recursion touches are committed.
const STACK: usize = 1 << 30;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut cp, mut chosen, mut target) = (Vec::new(), None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cp" | "--main" => {
                let Some(v) = args.next() else { return usage() };
                if a == "--cp" {
                    cp.push(PathBuf::from(v));
                } else {
                    chosen = Some(v);
                }
            }
            _ => {
                target = Some(a);
                break;
            }
        }
    }
    let Some(target) = target else { return usage() };
    let rest: Vec<String> = args.collect();
    let main_class = if script::is_source(&target) {
        match script::main_of(&target, chosen) {
            Ok((classes, main)) => {
                cp.push(classes);
                main
            }
            Err(e) => {
                eprintln!("scalai: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else if chosen.is_some() {
        return usage();
    } else {
        target
    };
    if let Some(secs) = std::env::var("SCALAI_TIMEOUT").ok().and_then(|s| s.parse().ok()) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            eprintln!("scalai: timed out after {secs} s");
            vm::exit_immediately(124);
        });
    }
    let thread = std::thread::Builder::new().stack_size(STACK).spawn(move || run(cp, &main_class, &rest));
    match thread.map_err(|e| e.to_string()).and_then(|t| t.join().map_err(|_| "interpreter panicked".into()))
    {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) | Err(e) => {
            eprintln!("scalai: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn run(mut cp: Vec<PathBuf>, main_class: &str, args: &[String]) -> Result<(), String> {
    let jars = libs::jars()?;
    let classpath = match Classpath::with_image(&cp, &jars, &libs::cache_dir()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("scalai: no library image ({e}); reading the jars");
            cp.extend(jars);
            Classpath::open(&cp).map_err(|e| e.to_string())?
        }
    };
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let result = vm.run_main(main_class, args);
    if std::env::var_os("SCALAI_PROFILE").is_some() {
        for (calls, name) in vm.profile_report(30) {
            eprintln!("{calls:>12}  {name}");
        }
    }
    let code = match result {
        Ok(()) => 0,
        Err(VmError::Exit(code)) => code,
        Err(e) => {
            eprintln!("scalai: {e}");
            1
        }
    };
    vm.exit(code)
}
