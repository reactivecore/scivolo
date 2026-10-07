//! A REPL session: one universe of symbols and one VM across the inputs, each input compiled
//! into wrapper object `rs$line$<N>` and run.

mod command;

use crate::earlier::{self, Earlier};
use crate::input::{self, Input, Parsed};
use crate::render::Definition;
use crate::{imports, line_names, render, report, trace, wrap};
use libimage::Image;
use lower::NirFile;
use std::panic::{AssertUnwindSafe, catch_unwind};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::Universe;
use typer::tpd::TNodeId;
use typer::{Fallback, Typed, Units};
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
    /// The lines of its warnings, printed before it runs.
    warnings: Vec<String>,
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
    /// `line` at the last reset.
    base: u32,
    /// The next free `res` index.
    res: u32,
    /// The earlier inputs that ran.
    earlier: Vec<Earlier>,
    exit: Option<i32>,
    /// The columns values are wrapped at.
    width: i32,
}

impl Session {
    /// A session running in `vm`, whose runtime is started here, over the TASTy of `image`,
    /// printing values in `width` columns.
    pub fn new(mut vm: Vm, image: &'static Image, width: i32) -> Result<Session, VmError> {
        vm.start_runtime("repl", &[])?;
        let (names, fresh) = (Names::default(), FreshNames::default());
        let (u, units) = (Universe::open_image(image), Units::default());
        let (earlier, exit) = (Vec::new(), None);
        Ok(Session { vm, image, u, units, names, fresh, line: 0, base: 0, res: 0, earlier, exit, width })
    }

    /// Forgets the definitions of all inputs; the VM keeps their code under names not reused.
    pub fn reset(&mut self) {
        self.u = Universe::open_image(self.image);
        self.units = Units::default();
        self.names = Names::default();
        self.fresh = FreshNames::default();
        self.base = self.line;
        self.res = 0;
        self.earlier.clear();
    }

    /// Makes Ctrl-C stop the running input instead of ending the process.
    pub fn watch_interrupts(&mut self) -> Result<(), VmError> {
        self.vm.watch_interrupts()
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
        out.iter().flat_map(|s| s.lines()).map(|l| line_names::shown(l, self.base)).collect()
    }

    /// Compiles input `text` into a unit to run, or the lines it prints instead.
    fn compile(&mut self, text: &str) -> Result<Unit, Vec<String>> {
        let name = format!("{}{}", line_names::PREFIX, self.line + 1);
        match input::classify(&name, text, &mut self.names, &mut self.fresh, self.width as usize) {
            Input::Empty => Err(Vec::new()),
            Input::Command(c) => Err(self.command(c)),
            Input::Errors(errors) => Err(errors),
            Input::Stats(parsed) => self.lower(*parsed),
        }
    }

    /// Types and lowers the statements of one input.
    fn lower(&mut self, mut parsed: Parsed) -> Result<Unit, Vec<String>> {
        self.line += 1;
        let object = format!("{}{}", line_names::PREFIX, self.line);
        let module = format!("{object}$");
        let (mut typed, template) = self.typed(&mut parsed, &object)?;
        let imports = imports::imports(&self.u, &typed, template, &parsed.source);
        let units = render::inferred_units(&mut self.u, &typed, template);
        let defs = render::definitions(&mut self.u, &module, &units);
        let members = earlier::members(&mut self.u, &module);
        let (names, fresh) = (&mut self.names, &mut self.fresh);
        let lowered = lower::lower(&mut self.u, &mut typed.tpd, typed.root, names, fresh, &parsed.source);
        let files = lowered.map_err(|e| vec![report::unsupported(&e.what)])?;
        let earlier = Earlier { object, imports, members };
        let warnings = report::warnings(&parsed.source, &parsed.diags, self.width as usize);
        Ok(Unit { module, files, defs, earlier, unit_res: render::results(&self.u, &units), warnings })
    }

    /// The statements of `parsed` typed as the body of wrapper object `object`, with the
    /// wrapper's template; the `res` index advances past them once they are wrapped.
    fn typed(&mut self, parsed: &mut Parsed, object: &str) -> Result<(Typed, TNodeId), Vec<String>> {
        let (names, fresh) = (&mut self.names, &mut self.fresh);
        let prelude = earlier::prelude(&self.earlier);
        let width = self.width as usize;
        let Some(wrapped) = wrap::wrap(parsed, object, self.res, &prelude, names, fresh) else {
            return Err(parsed.errors(Diagnostics::default(), width));
        };
        self.res = wrapped.next_res;
        let features = parsed.cfg.features;
        let typed =
            self.units.type_unit(&mut self.u, &mut parsed.ast, (names, fresh), wrapped.root, features);
        let mut typed = match typed {
            Ok(typed) => typed,
            Err(Fallback::Errors(diags)) => return Err(parsed.errors(diags, width)),
            Err(Fallback::Unsupported { what, .. }) => return Err(vec![report::unsupported(&what)]),
        };
        parsed.diags.items.append(&mut typed.diags.items);
        match wrap::template(&self.u, &typed, &format!("{object}$")) {
            Some(template) => Ok((typed, template)),
            None => Err(vec![report::unsupported("a wrapper object without a template")]),
        }
    }

    /// Loads and runs `unit`; the lines it prints.
    fn run(&mut self, unit: Unit) -> Vec<String> {
        let Unit { module, files, defs, earlier, unit_res, warnings: mut out } = unit;
        for f in files {
            if let Err(e) = self.vm.add_nir(&f.name, f.bytes) {
                out.push(e.to_string());
                return out;
            }
        }
        self.vm.clear_interrupts();
        if let Err(e) = self.vm.eval_module(&module) {
            out.extend(self.failed(e));
            return out;
        }
        self.res -= unit_res;
        self.earlier.push(earlier);
        for d in defs {
            match render::line(&mut self.vm, &module, &d, self.width) {
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
            VmError::Uncaught(exc) => trace::lines(&exc),
            VmError::Exit(code) => {
                self.exit = Some(code);
                Vec::new()
            }
            e => vec![e.to_string()],
        }
    }
}
