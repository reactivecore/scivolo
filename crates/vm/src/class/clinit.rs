//! Static initializers (`<clinit>`): the program's before `main`, the others on linking.

use crate::interp::{Flow, Unwound};
use crate::{Vm, VmError};
use nir::GlobalId;

const CLINIT: &str = "IE";

impl Vm {
    /// Runs the `<clinit>` of class `name`, if it has one.
    pub fn run_static_initializer(&mut self, name: GlobalId) -> Result<(), VmError> {
        let clinit = self.world.member(name, CLINIT);
        if self.world.defn(clinit).is_err() {
            return Ok(());
        }
        match self.call_global(clinit, &[]) {
            Ok(_) => Ok(()),
            Err(u) => match u.into_inner() {
                Unwound::Throw(exc) => Err(VmError::Initializer(exc)),
                Unwound::Fatal(e) => Err(e),
            },
        }
    }

    /// Runs the `<clinit>`s of the classes not linked yet whose files come before the library,
    /// sorted by name, as Scala Native runs every `<clinit>` before `main`; linking them later
    /// runs them no more.
    pub fn run_program_initializers(&mut self) -> Flow<()> {
        let mut clinits = Vec::new();
        for name in self.world.entry_point_tops() {
            let top = self.world.top(&name);
            let clinit = self.world.member(top, CLINIT);
            if !self.classes.by_name.contains_key(&top) && self.world.defn(clinit).is_ok() {
                clinits.push(clinit);
                self.classes.eager.insert(top);
            }
        }
        for clinit in clinits {
            self.call_global(clinit, &[])?;
        }
        Ok(())
    }
}
