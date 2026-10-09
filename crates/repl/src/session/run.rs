//! Running a compiled input: loading its NIR, constructing its wrapper module and printing its
//! definitions; in quiet mode only warnings and what the input prints itself.

use super::Session;
use crate::earlier::Earlier;
use crate::render::{self, Definition};
use crate::trace;
use lower::NirFile;
use vm::VmError;

/// A compiled input: its wrapper module, NIR files and definitions to print.
pub struct Unit {
    pub module: String,
    pub files: Vec<NirFile>,
    pub defs: Vec<Definition>,
    pub earlier: Earlier,
    /// How many `res` values of type `Unit` it defines, whose indices a run gives back.
    pub unit_res: u32,
    /// The lines of its warnings, printed before it runs.
    pub warnings: Vec<String>,
}

impl Session {
    /// Loads and runs `unit`; the lines it prints.
    pub fn run(&mut self, unit: Unit) -> Vec<String> {
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
                Ok(line) if !self.quiet => out.push(line),
                Ok(_) => {}
                Err(e) => {
                    out.extend(self.failed(e));
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
