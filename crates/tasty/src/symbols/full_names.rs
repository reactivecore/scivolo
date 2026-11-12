//! Comparing the qualified name of a symbol with a text without building it.

use super::{SymbolId, Universe};

impl Universe {
    /// Determines whether `full_name(s)` is `full`.
    pub fn has_full_name(&self, mut s: SymbolId, full: &str) -> bool {
        let mut rest = full;
        loop {
            let Some(prefix) = rest.strip_suffix(self.symbol(s).name.as_str()) else { return false };
            match self.symbol(s).owner.filter(|&o| o != self.root()) {
                Some(o) => {
                    let Some(prefix) = prefix.strip_suffix('.') else { return false };
                    rest = prefix;
                    s = o;
                }
                None => return prefix.is_empty(),
            }
        }
    }

    /// Determines whether `full_name(s)` starts with `prefix`.
    pub fn full_name_starts_with(&self, s: SymbolId, prefix: &str) -> bool {
        self.after_full_name(s, prefix) == Some("")
    }

    /// What remains of `prefix` after `full_name(s)`: empty once the name covers it, none where
    /// they differ.
    fn after_full_name<'p>(&self, s: SymbolId, prefix: &'p str) -> Option<&'p str> {
        let rest = match self.symbol(s).owner.filter(|&o| o != self.root()) {
            Some(o) => match self.after_full_name(o, prefix)? {
                "" => return Some(""),
                rest => rest.strip_prefix('.')?,
            },
            None => prefix,
        };
        let name = self.symbol(s).name.as_str();
        match rest.len() <= name.len() {
            true => name.starts_with(rest).then_some(""),
            false => rest.strip_prefix(name),
        }
    }
}
