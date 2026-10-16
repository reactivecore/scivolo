mod frontend;
mod history;
mod mains;
mod repl;
mod script;
mod tab;

use clap::{CommandFactory, Parser, error::ErrorKind};
use std::path::PathBuf;
use std::process::ExitCode;
use vm::{Classpath, Vm, VmError};

/// Runs a Scala program: a main class on the classpath, a Scala file or a script; without a
/// target, starts the REPL.
///
/// Environment: SCALAI_LIBS holds the library jars (default: the Coursier cache);
/// SCALAI_CACHE holds the library image and compiled scripts; SCALAI_MACROS=on expands macro
/// calls in scripts and the REPL, running them in the VM; SCALAI_FRONTEND=strict compiles
/// scripts with the own frontend only, =jvm with scala-cli only (default: the own frontend,
/// scala-cli where it refuses); SCALAI_SCALA_CLI names the scala-cli binary; SCALAI_TIMEOUT
/// stops the program, compiling included, after that many seconds (exit 124); SCALAI_TRACE traces exceptions;
/// SCALAI_PROFILE prints the most called methods to stderr; RUST_BACKTRACE prints panics in
/// full. The REPL keeps its history in ~/.dotty_history, as Scala's own REPL does.
#[derive(Parser)]
#[command(name = "scalai", override_usage = "scalai [OPTIONS] [TARGET] [ARGS]...")]
struct Cli {
    /// A directory or jar of NIR files and resources.
    #[arg(long, value_name = "DIR_OR_JAR")]
    cp: Vec<PathBuf>,
    /// A Scala Native library from Maven, with what it depends on, fetched by Coursier onto the
    /// classpath: `group::artifact::version`, `group::artifact:version` or
    /// `group:artifact:version`; repeatable.
    #[arg(long = "dep", value_name = "COORDINATE")]
    deps: Vec<String>,
    /// The main class of FILE.scala, when it has several.
    #[arg(long, value_name = "CLASS")]
    main: Option<String>,
    /// A Scala compiler option for FILE.scala, such as -Werror; repeatable.
    #[arg(short = 'O', long = "scala-opt", value_name = "OPT", allow_hyphen_values = true)]
    scala_opt: Vec<String>,
    /// MAIN_CLASS, FILE.scala or FILE.sc, then the program's arguments.
    #[arg(value_name = "TARGET", trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// The interpreter thread's stack; only the pages deep recursion touches are committed.
const STACK: usize = 1 << 30;

fn main() -> ExitCode {
    let Cli { mut cp, deps, main: chosen, scala_opt, command: mut rest } = Cli::parse();
    let target = (!rest.is_empty()).then(|| rest.remove(0));
    let source = target.as_deref().is_some_and(script::is_source);
    if chosen.is_some() && !source {
        Cli::command().error(ErrorKind::ArgumentConflict, "--main needs FILE.scala").exit()
    }
    if !scala_opt.is_empty() && !source {
        Cli::command().error(ErrorKind::ArgumentConflict, "--scala-opt needs FILE.scala").exit()
    }
    match libs::deps::fetch(&deps) {
        Ok(jars) => cp.extend(jars),
        Err(e) => {
            eprintln!("scalai: {e}");
            return ExitCode::FAILURE;
        }
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| match std::env::var_os("RUST_BACKTRACE") {
        Some(_) => default_hook(info),
        None => eprintln!("scalai: panic at {}", info.location().map_or("?".into(), |l| l.to_string())),
    }));
    let thread = std::thread::Builder::new().stack_size(STACK).spawn(move || match target {
        Some(target) => run(cp, target, (chosen, &scala_opt), &rest),
        None => {
            watch_timeout();
            repl::run(&cp)
        }
    });
    let joined = thread
        .map_err(|e| e.to_string())
        .and_then(|t| t.join().map_err(|p| lower::output::panicked(p.as_ref())));
    match joined {
        Ok(Ok(code)) => ExitCode::from(code as u8),
        Ok(Err(e)) | Err(e) => {
            eprintln!("scalai: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Stops the process after `SCALAI_TIMEOUT` seconds, if set.
fn watch_timeout() {
    if let Some(secs) = std::env::var("SCALAI_TIMEOUT").ok().and_then(|s| s.parse().ok()) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            eprintln!("scalai: timed out after {secs} s");
            vm::exit_immediately(124);
        });
    }
}

/// Runs main class `target`, or the main class of source file `target` (`chosen` if given)
/// compiled with compiler options `options`.
fn run(
    cp: Vec<PathBuf>,
    target: String,
    (chosen, options): (Option<String>, &[String]),
    args: &[String],
) -> Result<i32, String> {
    watch_timeout();
    let jars = libs::jars()?;
    let with_cp: Vec<PathBuf> = jars.iter().chain(cp.iter().filter(|p| !p.is_dir())).cloned().collect();
    let image = libs::image(&with_cp);
    let known = image.as_ref().ok().copied();
    let classpath = match image.and_then(|image| Classpath::with_image(&cp, image)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("scalai: no library image ({e}); reading the jars");
            let all: Vec<PathBuf> = cp.iter().chain(&jars).cloned().collect();
            Classpath::open(&all).map_err(|e| e.to_string())?
        }
    };
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let main_class = match script::is_source(&target) {
        true => {
            let (classes, main) = script::main_of(&target, chosen, (known, &cp), options, &mut vm)?;
            vm.world.add_classpath(&classes).map_err(|e| format!("{}: {e}", classes.display()))?;
            main
        }
        false => target,
    };
    let result = vm.run_main(&main_class, args);
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
