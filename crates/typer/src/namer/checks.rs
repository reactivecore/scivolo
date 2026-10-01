//! Definitions outside the subset, or that dotty's checks (`Checking`, `RefChecks`) reject and the
//! typer does not report yet: both fall back.

use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Checks definition `tree` of `name` with dotty flags `flags` in `owner`, before its symbol
    /// is entered.
    pub fn check_definition(
        &mut self,
        tree: NodeId,
        name: &str,
        kind: Kind,
        owner: SymbolId,
        flags: FlagSet,
    ) {
        if flags.is(f::INLINE) {
            self.unsupported(tree, "inline definition");
        } else if flags.is(f::PRIVATE) && flags.is_one_of(f::OVERRIDE.or(f::ABS_OVERRIDE)) {
            self.unsupported(tree, "private and override");
        } else if flags.is(f::PRIVATE) && self.u.symbol(owner).kind == Kind::Package {
            self.unsupported(tree, "private top-level definition");
        } else if kind != Kind::Def && self.defines(owner, name, kind == Kind::Class) {
            self.unsupported(tree, "double definition");
        } else if kind == Kind::Class
            && self.u.symbol(owner).kind == Kind::Class
            && self.inherits_class(owner, name)
        {
            self.unsupported(tree, "class shadowing an inherited class");
        }
    }

    /// Whether this unit defined a term (or type) named `name` in `owner` already.
    fn defines(&self, owner: SymbolId, name: &str, is_type: bool) -> bool {
        let decls = &self.u.symbol(owner).decls;
        decls.iter().any(|&d| {
            self.syms.flags.contains_key(&d)
                && self.u.symbol(d).name == name
                && crate::symbols::is_type_symbol(self.u, d) == is_type
        })
    }

    /// Whether a base class of class `c` other than `c` has a member class named `name`.
    fn inherits_class(&mut self, c: SymbolId, name: &str) -> bool {
        let bases = self.u.linearization(c);
        bases.iter().skip(1).any(|&b| {
            let decls = &self.u.symbol(b).decls;
            decls.iter().any(|&d| self.u.symbol(d).kind == Kind::Class && self.u.symbol(d).name == name)
        })
    }

    /// Checks that annotation class `annot` is one the typer handles: `@main`.
    pub fn check_annotation(&mut self, annot: NodeId, class: Option<SymbolId>) {
        if class != Some(self.defn.main_annot) {
            self.unsupported(annot, "annotation");
        }
    }
}
