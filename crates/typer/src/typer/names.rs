//! The unit's interned names, and the builder of typed trees over them.

use super::Typer;
use crate::tpd::Build;
use syntax::names::Name;

impl Typer<'_> {
    pub fn name(&mut self, text: &str) -> Name {
        self.names.intern(text)
    }

    pub fn text(&self, name: Name) -> &str {
        self.names.text(name)
    }

    pub fn build(&mut self) -> Build<'_> {
        Build::new(&mut self.tpd, self.u, self.names, &self.defn)
    }
}
