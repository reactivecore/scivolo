//! A REPL session: one universe of symbols and one VM across the inputs, each input compiled
//! into wrapper object `rs$line$<N>` and run.

use crate::commands::{self, Command};
use crate::earlier::{self, Earlier};
use crate::input::{self, Input, Parsed};
use crate::render::Definition;
use crate::{imports, render, report, wrap};
use libimage::Image;
use lower::NirFile;
use std::panic::{AssertUnwindSafe, catch_unwind};
use syntax::names::{FreshNames, Names};
use tasty::Universe;
use typer::{Fallback, Units};
use vm::{Vm, VmError};

const RESETTING: &str = "Resetting REPL state.";

/// A compiled input: its wrapper module, NIR files and definitions to print.
struct Unit {
    module: String,
    files: Vec<NirFile>,
    defs: Vec<Definition>,
    earlier: Earlier,
    /// How many `res` values of type `Unit` it defines, whose indices a run gives back.
    unit_res: u32,
}

pub struct Session {
    vm: Vm,
    image: &'static Image,
    u: Universe,
    units: Units,
    names: Names,
    fresh: FreshNames,
    /// The index of the last wrapper object; it grows across resets.
    line: u32,
    /// The next free `res` index.
    res: u32,
    /// The earlier inputs that ran.
    earlier: Vec<Earlier>,
    exit: Option<i32>,
}

impl Session {
    /// A session running in `vm`, whose runtime is started here, over the TASTy of `image`.
    pub fn new(mut vm: Vm, image: &'static Image) -> Result<Session, VmError> {
        vm.start_runtime("repl", &[])?;
        let (names, fresh) = (Names::default(), FreshNames::default());
        let (u, units) = (Universe::open_image(image), Units::default());
        let earlier = Vec::new();
        Ok(Session { vm, image, u, units, names, fresh, line: 0, res: 0, earlier, exit: None })
    }

    /// Forgets the definitions of all inputs; the VM keeps their code under names not reused.
    pub fn reset(&mut self) {
        self.u = Universe::open_image(self.image);
        self.units = Units::default();
        self.names = Names::default();
        self.fresh = FreshNames::default();
        self.res = 0;
        self.earlier.clear();
    }

    /// The exit code an input ended the session with: 0 after `:quit`, `n` after `sys.exit(n)`.
    pub fn exit(&self) -> Option<i32> {
        self.exit
    }

    /// Ends the session: the runtime's shutdown runs queued tasks.
    pub fn shutdown(mut self) -> Result<(), VmError> {
        self.vm.shutdown()
    }

    /// Evaluates input `text`; the lines it prints. A panic while compiling the input is reported
    /// as an internal error and resets the session; a panic while running it is passed on.
    pub fn eval(&mut self, text: &str) -> Vec<String> {
        let compiled = catch_unwind(AssertUnwindSafe(|| self.compile(text)));
        let out = match compiled {
            Ok(Ok(unit)) => self.run(unit),
            Ok(Err(out)) => out,
            Err(panic) => {
                self.reset();
                vec![report::internal(panic.as_ref()), RESETTING.to_owned()]
            }
        };
        out.iter().flat_map(|s| s.lines()).map(str::to_owned).collect()
    }

    /// Compiles input `text` into a unit to run, or the lines it prints instead.
    fn compile(&mut self, text: &str) -> Result<Unit, Vec<String>> {
        let name = format!("rs$line${}", self.line + 1);
        match input::classify(&name, text, &mut self.names, &mut self.fresh) {
            Input::Empty => Err(Vec::new()),
            Input::Command(c) => Err(self.command(c)),
            Input::Errors(errors) => Err(errors),
            Input::Stats(parsed) => self.lower(parsed),
        }
    }

    fn command(&mut self, c: Command) -> Vec<String> {
        match c {
            Command::Quit => {
                self.exit = Some(0);
                Vec::new()
            }
            Command::Help => vec![commands::HELP.to_owned()],
            Command::Reset(_) => {
                self.reset();
                vec![RESETTING.to_owned()]
            }
            Command::Unknown(name) => vec![commands::unknown(&name)],
        }
    }

    /// Types and lowers the statements of one input.
    fn lower(&mut self, mut parsed: Parsed) -> Result<Unit, Vec<String>> {
        self.line += 1;
        let object = format!("rs$line${}", self.line);
        let (names, fresh) = (&mut self.names, &mut self.fresh);
        let prelude = earlier::prelude(&self.earlier);
        let wrapped = wrap::wrap(&mut parsed, &object, self.res, &prelude, names, fresh)?;
        self.res = wrapped.next_res;
        let features = parsed.cfg.features;
        let typed = self.units.type_unit(&mut self.u, &mut parsed.ast, names, wrapped.root, features);
        let mut typed = match typed {
            Ok(typed) => typed,
            Err(Fallback::Errors(diags)) => return Err(report::diagnostics(&parsed.source, &diags)),
            Err(Fallback::Unsupported { what, .. }) => return Err(vec![report::unsupported(&what)]),
        };
        let module = format!("{object}$");
        let Some(template) = wrap::template(&self.u, &typed, &module) else {
            return Err(vec![report::unsupported("a wrapper object without a template")]);
        };
        let imports = imports::imports(&self.u, &typed, template, &parsed.source);
        let units = render::inferred_units(&mut self.u, &typed, template);
        let defs = render::definitions(&mut self.u, &module, &units);
        let members = earlier::members(&mut self.u, &module);
        let lowered = lower::lower(&mut self.u, &mut typed.tpd, typed.root, names, fresh, &parsed.source);
        let files = lowered.map_err(|e| vec![report::unsupported(&e.what)])?;
        let earlier = Earlier { object, imports, members };
        Ok(Unit { module, files, defs, earlier, unit_res: render::results(&self.u, &units) })
    }

    /// Loads and runs `unit`; the lines it prints.
    fn run(&mut self, unit: Unit) -> Vec<String> {
        let Unit { module, files, defs, earlier, unit_res } = unit;
        for f in files {
            if let Err(e) = self.vm.add_nir(&f.name, f.bytes) {
                return vec![e.to_string()];
            }
        }
        if let Err(e) = self.vm.eval_module(&module) {
            return self.failed(e);
        }
        self.res -= unit_res;
        self.earlier.push(earlier);
        let mut out = Vec::new();
        for d in defs {
            match render::line(&mut self.vm, &module, &d) {
                Ok(line) => out.push(line),
                Err(e) => {
                    out.extend(self.failed(e));
                    break;
                }
            }
        }
        out
    }

    /// What a run that failed with `e` prints.
    fn failed(&mut self, e: VmError) -> Vec<String> {
        match e {
            VmError::Uncaught(exc) => vec![exc],
            VmError::Exit(code) => {
                self.exit = Some(code);
                Vec::new()
            }
            e => vec![e.to_string()],
        }
    }
}
