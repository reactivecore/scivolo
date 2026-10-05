//! The unit's interned names.

use super::Typer;
use syntax::names::Name;

impl Typer<'_> {
    pub fn name(&mut self, text: &str) -> Name {
        self.names.intern(text)
    }

    pub fn text(&self, name: Name) -> &str {
        self.names.text(name)
    }
}
