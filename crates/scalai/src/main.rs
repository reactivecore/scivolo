mod libs;

use std::path::PathBuf;
use std::process::ExitCode;
use vm::{Classpath, Vm};

const USAGE: &str = "usage: scalai [--cp DIR_OR_JAR]... MAIN_CLASS [ARGS...]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut cp = Vec::new();
    let mut main_class = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cp" => match args.next() {
                Some(p) => cp.push(PathBuf::from(p)),
                None => return usage(),
            },
            _ => {
                main_class = Some(a);
                break;
            }
        }
    }
    let Some(main_class) = main_class else { return usage() };
    let rest: Vec<String> = args.collect();
    match run(cp, &main_class, &rest) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
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
    cp.extend(libs::jars()?);
    let classpath = Classpath::open(&cp).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    vm.run_main(main_class, args).map_err(|e| e.to_string())
}
