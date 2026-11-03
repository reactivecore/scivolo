//! Running a compiled input: loading its NIR, constructing its wrapper module and printing its
//! definitions and warnings by place; in quiet mode only warnings and what the input prints itself.

use super::Session;
use crate::earlier::Earlier;
use crate::render::{self, Definition};
use crate::report::Place;
use crate::trace;
use lower::NirFile;
use vm::VmError;

/// The place of the wrapper module, whose span starts the input.
const WRAPPER: Place = (0, true, 0);

/// A compiled input: its wrapper module, NIR files and definitions to print.
pub struct Unit {
    pub module: String,
    pub files: Vec<NirFile>,
    pub defs: Vec<(Place, Definition)>,
    pub earlier: Earlier,
    /// How many `res` values of type `Unit` it defines, whose indices a run gives back.
    pub unit_res: u32,
    /// The count of its warnings, printed after its own output.
    pub count: Vec<String>,
    /// The lines of each warning, printed among its definitions by place.
    pub warnings: Vec<(Place, Vec<String>)>,
}

impl Session {
    /// Loads and runs `unit`; the lines it prints.
    pub fn run(&mut self, unit: Unit) -> Vec<String> {
        let Unit { module, files, defs, earlier, unit_res, count: mut out, mut warnings } = unit;
        for f in files {
            if let Err(e) = self.vm.add_nir(&f.name, f.bytes) {
                out.extend(warnings.into_iter().flat_map(|(_, w)| w));
                out.push(e.to_string());
                return out;
            }
        }
        self.vm.clear_interrupts();
        if let Err(e) = self.vm.eval_module(&module) {
            let init = defs.iter().position(|(_, d)| d.initializes()).unwrap_or(defs.len());
            let lazies = defs[..init].iter().filter(|(_, d)| d.before_init() && !self.quiet);
            warnings.extend(lazies.map(|(p, d)| (*p, vec![d.text().to_owned()])));
            warnings.push((defs.get(init).map_or(WRAPPER, |(p, _)| *p), self.failed(e)));
        } else {
            self.res -= unit_res;
            self.earlier.push(earlier);
            warnings.extend(self.rendered(&module, defs));
        }
        warnings.sort_by_key(|(p, _)| *p);
        out.extend(warnings.into_iter().flat_map(|(_, w)| w));
        out
    }

    /// The lines of `defs` of module `module` by place, up to the first that fails.
    fn rendered(&mut self, module: &str, defs: Vec<(Place, Definition)>) -> Vec<(Place, Vec<String>)> {
        let mut out = Vec::new();
        for (p, d) in defs.into_iter().filter(|(_, d)| d.printed) {
            match render::line(&mut self.vm, module, &d, self.width) {
                Ok(line) if !self.quiet => out.push((p, vec![line])),
                Ok(_) => {}
                Err(e) => {
                    out.push((p, self.failed(e)));
                    break;
                }
            }
        }
        out
    }

    /// What a run that failed with `e` prints; in quiet mode not the trace of an exception.
    fn failed(&mut self, e: VmError) -> Vec<String> {
        match e {
            VmError::Uncaught(_) if self.quiet => Vec::new(),
            VmError::Uncaught(exc) => trace::lines(&exc),
            VmError::Exit(code) => {
                self.exit = Some(code);
                Vec::new()
            }
            e => vec![e.to_string()],
        }
    }
}
