mod libs;
mod repl;
mod script;

use clap::{CommandFactory, Parser, error::ErrorKind};
use libimage::Image;
use std::path::PathBuf;
use std::process::ExitCode;
use vm::{Classpath, Vm, VmError};

/// Runs a Scala program: a main class on the classpath, a Scala file or a script; without a
/// target, starts the REPL.
///
/// Environment: SCALAI_LIBS holds the library jars (default: the Coursier cache);
/// SCALAI_CACHE holds the library image, compiled scripts and the REPL history; SCALAI_SCALA_CLI
/// names the scala-cli binary; SCALAI_TIMEOUT stops the program after that many seconds (exit
/// 124); SCALAI_TRACE traces exceptions; SCALAI_PROFILE prints the most called methods to stderr.
#[derive(Parser)]
#[command(name = "scalai", override_usage = "scalai [OPTIONS] [TARGET] [ARGS]...")]
struct Cli {
    /// A directory or jar of NIR files and resources.
    #[arg(long, value_name = "DIR_OR_JAR")]
    cp: Vec<PathBuf>,
    /// The main class of FILE.scala, when it has several.
    #[arg(long, value_name = "CLASS")]
    main: Option<String>,
    /// MAIN_CLASS, FILE.scala or FILE.sc, then the program's arguments.
    #[arg(value_name = "TARGET", trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// The interpreter thread's stack; only the pages deep recursion touches are committed.
const STACK: usize = 1 << 30;

fn main() -> ExitCode {
    let Cli { mut cp, main: chosen, command: mut rest } = Cli::parse();
    let main_class = match rest.is_empty() {
        true if chosen.is_some() => {
            Cli::command().error(ErrorKind::ArgumentConflict, "--main needs FILE.scala").exit()
        }
        true => None,
        false => {
            let target = rest.remove(0);
            match target_class(target, chosen, &mut cp) {
                Ok(main) => Some(main),
                Err(e) => {
                    eprintln!("scalai: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    if let Some(secs) = std::env::var("SCALAI_TIMEOUT").ok().and_then(|s| s.parse().ok()) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            eprintln!("scalai: timed out after {secs} s");
            vm::exit_immediately(124);
        });
    }
    let thread = std::thread::Builder::new().stack_size(STACK).spawn(move || match main_class {
        Some(main_class) => run(cp, &main_class, &rest),
        None => repl::run(&cp),
    });
    match thread.map_err(|e| e.to_string()).and_then(|t| t.join().map_err(|_| "interpreter panicked".into()))
    {
        Ok(Ok(code)) => ExitCode::from(code as u8),
        Ok(Err(e)) | Err(e) => {
            eprintln!("scalai: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The main class of `target` given `--main` `chosen`; a source file's classes join `cp`.
fn target_class(target: String, chosen: Option<String>, cp: &mut Vec<PathBuf>) -> Result<String, String> {
    if script::is_source(&target) {
        let (classes, main) = script::main_of(&target, chosen)?;
        cp.push(classes);
        Ok(main)
    } else if chosen.is_some() {
        Cli::command().error(ErrorKind::ArgumentConflict, "--main needs FILE.scala").exit()
    } else {
        Ok(target)
    }
}

/// A VM over `cp` and the library jars, and their image if there is one.
fn vm(cp: &[PathBuf]) -> Result<(Vm, Option<&'static Image>), String> {
    let jars = libs::jars()?;
    let (classpath, image) = match libs::image(&jars).and_then(|i| Ok((Classpath::with_image(cp, i)?, i))) {
        Ok((c, image)) => (c, Some(image)),
        Err(e) => {
            eprintln!("scalai: no library image ({e}); reading the jars");
            let all: Vec<PathBuf> = cp.iter().cloned().chain(jars).collect();
            (Classpath::open(&all).map_err(|e| e.to_string())?, None)
        }
    };
    Ok((Vm::new(classpath).map_err(|e| e.to_string())?, image))
}

fn run(cp: Vec<PathBuf>, main_class: &str, args: &[String]) -> Result<i32, String> {
    let (mut vm, _) = vm(&cp)?;
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
