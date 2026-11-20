//! A whole `.scala` file compiled by the own frontend and its main method run, as
//! `scivolo FILE.scala` runs it, without its cache and its scala-cli fallback.

use crate::{report, script};
use libimage::Image;
use lower::NirFile;
use lower::directives::{directives, without_header};
use macros::{Macros, Settings, VmMacros};
use std::panic::{AssertUnwindSafe, catch_unwind};
use syntax::config::Config;
use syntax::diag::{Diagnostics, Level};
use syntax::source::SourceFile;
use tasty::Universe;
use vm::{Vm, VmError};

/// The exit code of a program that does not compile or ends with an error, as `scivolo`'s.
const FAILED: i32 = 1;

/// What running a program printed and the code it exited with.
pub struct Ran {
    /// Its compiler messages, what it wrote to stdout and stderr where the VM keeps that
    /// (portable mode), then the error it ended with.
    pub lines: Vec<String>,
    pub exit: i32,
    /// The lines, from 1, that its compile errors point at.
    pub error_lines: Vec<usize>,
}

impl Ran {
    fn failed(lines: Vec<String>, error_lines: Vec<usize>) -> Ran {
        Ran { lines, exit: FAILED, error_lines }
    }
}

/// A program compiled to NIR: its files, main class and the lines of its warnings.
pub struct Program {
    pub files: Vec<NirFile>,
    pub main: String,
    pub warnings: Vec<String>,
}

/// Compiles `text`, the source of file `name`, as `scivolo FILE.scala` does with its own
/// frontend: against the library TASTy of `image`, with the options of its `//> using`
/// directives, its macros run in `vm`. A file with top-level statements is compiled as a `.sc`
/// script, as scala-cli wraps it (`script`). Compiler messages are drawn `width` columns wide. Fails
/// with what a run prints: syntax and type errors, warnings under `-Werror`, and refusals of
/// files with dependencies, directives other than options and the Scala version, constructs the
/// frontend does not support, and several or no main classes (`@main` methods or objects with
/// `def main(args: Array[String])`). A program whose compile ran a macro (`vm.macro_expansions`)
/// must run in another VM, so that the macros' side effects do not reach it (ADR 0029).
pub fn compile_program(
    (name, text): (&str, &str),
    image: &'static Image,
    width: usize,
    vm: &mut Vm,
) -> Result<Program, Ran> {
    let refused = |what: String| Ran::failed(vec![report::unsupported(&what)], Vec::new());
    let found = directives(text, libimage::SCALA).map_err(|d| refused(format!("the directive {d}")))?;
    if let Some((_, dep)) = found.deps.first() {
        return Err(refused(format!("dependency {dep}: no libraries can be fetched here")));
    }
    let mut options = found.options;
    let settings = Settings::from_options(&options);
    options.retain(|o| !Settings::reads(o));
    let cfg = Config::checked(&options).map_err(|o| refused(format!("option `{o}`")))?;

    let text = without_header(text);
    let source = SourceFile::new(name, text.clone());
    let mut runner = VmMacros { vm, sources: Vec::new(), settings };
    let mut lowered = lower((name, text.clone()), &mut runner, image, cfg);
    if matches!(&lowered, Ok(Err(r)) if script::has_statements(&r.diags)) {
        lowered = lower((name, script::wrap(&text)), &mut runner, image, cfg);
        let diags = match &mut lowered {
            Ok(Ok((_, warnings))) => Some(warnings),
            Ok(Err(r)) => Some(&mut r.diags),
            Err(_) => None,
        };
        diags.into_iter().for_each(|d| script::unwrap(d, text.len()));
    }
    let (files, warnings) = match lowered {
        Err(panic) => return Err(Ran::failed(vec![report::internal(panic.as_ref())], Vec::new())),
        Ok(Err(r)) if r.diags.items.is_empty() => return Err(refused(r.what)),
        Ok(Err(r)) => {
            let lines = report::compile_errors(&source, &r.diags, width);
            return Err(Ran::failed(lines, lines_of(&source, &r.diags, Level::Error)));
        }
        Ok(Ok(lowered)) => lowered,
    };

    let mut shown = Vec::new();
    if !cfg.warnings.nowarn && !warnings.items.is_empty() {
        shown = report::compile_errors(&source, &warnings, width);
        if cfg.warnings.fatal {
            let at = lines_of(&source, &warnings, Level::Warning);
            return Err(Ran::failed(report::fatal_warnings(shown), at));
        }
    }
    let main = only_main(name, &files).map_err(|e| Ran::failed(vec![e], Vec::new()))?;
    Ok(Program { files, main, warnings: shown })
}

/// What compiling a file gives: its NIR files and warnings, why it does not compile, or the
/// compiler's panic.
type Lowered = std::thread::Result<Result<(Vec<NirFile>, Diagnostics), lower::Refusal>>;

/// Compiles the file `name` holding `text` against the library TASTy of `image`, its macros run
/// in `runner`.
fn lower(
    (name, text): (&str, String),
    runner: &mut VmMacros<'_>,
    image: &'static Image,
    cfg: Config,
) -> Lowered {
    let source = SourceFile::new(name, text.clone());
    runner.sources = vec![SourceFile::new(name, text)];
    catch_unwind(AssertUnwindSafe(|| {
        let macros = Some(runner as &mut dyn Macros);
        lower::compile_with(&mut Universe::open_image(image), &source, cfg, macros)
    }))
}

/// Finds the one main class among `files` compiled from file `name`.
fn only_main(name: &str, files: &[NirFile]) -> Result<String, String> {
    match &lower::output::mains(files)?[..] {
        [main] => Ok(main.clone()),
        [] => Err(format!("{name}: no main method")),
        several => Err(format!("{name}: several main classes: {}", several.join(", "))),
    }
}

/// The lines, from 1, of the diagnostics of `level` in `diags`, sorted, each once.
fn lines_of(source: &SourceFile, diags: &Diagnostics, level: Level) -> Vec<usize> {
    let at = diags.items.iter().filter(|d| d.level == level).filter_map(|d| d.offset);
    let mut lines: Vec<usize> =
        at.filter(|&o| o <= source.len()).map(|o| source.offset_to_line(o) + 1).collect();
    lines.sort();
    lines.dedup();
    lines
}

/// Loads `program` into `vm` and runs its `main` without arguments within the VM's time limit,
/// as `scivolo` runs a main class: runtime initialization, `main`, the runtime's shutdown. An
/// error it ends with is printed as `scivolo` prints it.
pub fn run_program(vm: &mut Vm, program: Program) -> Ran {
    let Program { files, main, warnings: mut lines } = program;
    for f in files {
        if let Err(e) = vm.add_nir(&f.name, f.bytes) {
            lines.push(e.to_string());
            return Ran::failed(lines, Vec::new());
        }
    }

    vm.clear_interrupts();
    vm.start_time_limit();
    let result = vm.run_main(&main, &[]);
    vm.end_time_limit();

    let written: Vec<u8> = vm.take_output().into_iter().flat_map(|w| w.bytes).collect();
    lines.extend(String::from_utf8_lossy(&written).lines().map(str::to_owned));
    let exit = match result {
        Ok(()) => 0,
        Err(VmError::Exit(code)) => code,
        Err(e) => {
            lines.extend(format!("scivolo: {e}").lines().map(str::to_owned));
            FAILED
        }
    };
    Ran { lines, exit, error_lines: Vec::new() }
}
