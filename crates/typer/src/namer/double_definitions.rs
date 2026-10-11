//! Definitions entered twice in one owner (dotty's `Namer.checkNoConflict`, and in a class
//! `Checking.checkNoDoubleDeclaration` as far as values go).

use crate::typer::Typer;
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Whether definition `tree` of `name` and `kind` conflicts with a definition this unit entered
    /// in `owner` already. Two methods are left to `check_overrides`; in a class, a value and a
    /// method conflict only when the method is nullary.
    pub fn doubly_defined(&mut self, tree: NodeId, name: &str, kind: Kind, owner: SymbolId) -> bool {
        if name == "$anon" {
            return false;
        }
        let is_type = !kind.is_term();
        let in_class = self.u.symbol(owner).kind == Kind::Class;
        let getter = name.contains("$default$");
        let decls = self.u.symbol(owner).decls.clone();
        for d in decls {
            let s = self.u.symbol(d);
            if !self.syms.flags.contains_key(&d) || s.name != name || self.u.is_type_symbol(d) != is_type {
                continue;
            }
            let conflicts = match (kind, s.kind) {
                _ if is_type || getter => true,
                (Kind::Def, Kind::Val) => !in_class || self.is_nullary_def(tree),
                (Kind::Def, _) => false,
                (_, Kind::Def) => !in_class || self.is_nullary(d),
                _ => true,
            };
            if conflicts {
                return true;
            }
        }
        false
    }
}
