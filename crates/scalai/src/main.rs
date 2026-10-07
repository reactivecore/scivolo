mod libs;
mod script;

use clap::{CommandFactory, Parser, error::ErrorKind};
use std::path::PathBuf;
use std::process::ExitCode;
use vm::{Classpath, Vm, VmError};

/// Runs a Scala program: a main class on the classpath, a Scala file or a script.
///
/// Environment: SCALAI_TIMEOUT stops the program after that many seconds (exit 124);
/// SCALAI_PROFILE prints the most called methods to stderr.
#[derive(Parser)]
#[command(name = "scalai", override_usage = "scalai [OPTIONS] <TARGET> [ARGS]...")]
struct Cli {
    /// A directory or jar of NIR files and resources.
    #[arg(long = "cp", value_name = "DIR_OR_JAR")]
    cp: Vec<PathBuf>,
    /// The main class of FILE.scala, when it has several.
    #[arg(long, value_name = "CLASS")]
    main: Option<String>,
    /// MAIN_CLASS, FILE.scala or FILE.sc, then the program's arguments.
    #[arg(value_name = "TARGET", required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// The interpreter thread's stack; only the pages deep recursion touches are committed.
const STACK: usize = 1 << 30;

fn main() -> ExitCode {
    let Cli { mut cp, main: chosen, mut command } = Cli::parse();
    let rest = command.split_off(1);
    let target = command.remove(0);
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
        Cli::command().error(ErrorKind::ArgumentConflict, "--main needs FILE.scala").exit()
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

fn run(mut cp: Vec<PathBuf>, main_class: &str, args: &[String]) -> Result<(), String> {
    let jars = libs::jars()?;
    let classpath = match libs::image(&jars).and_then(|image| Classpath::with_image(&cp, image)) {
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
