//! Inheritance that dotty rejects or the subset leaves out: both fall back.

use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags as f;
use tasty::tags::{CASE, ENUM, FINAL, LAZY, OBJECT, SEALED, TRAIT};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Checks the parent types of class `cls` with template `rhs`: no final class, no
    /// sealed class of the library, no case class extending a case class, no cycle.
    pub fn check_parents(&mut self, cls: SymbolId, rhs: NodeId, parents: &[tasty::TypeId]) {
        let case = self.syms.flags.get(&cls).is_some_and(|fl| fl.is(f::CASE));
        for &p in parents {
            let Some(c) = self.u.class_symbol(p) else { continue };
            let flags = self.u.symbol(c).flags;
            let case_base = |t: &mut Self| {
                t.u.linearization(c).into_iter().any(|b| {
                    let fl = t.u.symbol(b).flags;
                    fl.has(CASE) && !fl.has(TRAIT)
                })
            };
            let tycon = match *self.u.types().get(p) {
                tasty::Type::Applied(t, _) => t,
                _ => p,
            };
            let prefix = match *self.u.types().get(tycon) {
                tasty::Type::Sym(pre, _) => pre,
                tasty::Type::TypeRef(pre, _) => Some(pre),
                _ => None,
            };
            let projection = prefix.is_some_and(|pre| {
                matches!(*self.u.types().get(pre), tasty::Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class)
            });
            let def_span = |t: &Self, s: SymbolId| t.syms.def_trees.get(&s).map(|&d| t.ast.span(d));
            if flags.has(SEALED)
                && let (Some(parent), Some(at)) = (def_span(self, c), def_span(self, cls))
                && self.file_of(parent) != self.file_of(at)
            {
                let kind = if flags.has(TRAIT) { "trait" } else { "class" };
                let name = &self.u.symbol(c).name;
                let msg = format!("Cannot extend sealed {kind} {name} in a different source file");
                self.diags.error(at.point(), syntax::diag::Message::syntax(112, msg));
                continue;
            }
            let what = if c == cls || self.u.derives_from(c, cls) {
                "cyclic inheritance"
            } else if projection {
                "parent with an unstable prefix"
            } else if case && case_base(self) {
                "case class extending a case class"
            } else if (flags.has(FINAL) && !(flags.has(ENUM) && self.is_derived_value_class(c)))
                || flags.has(OBJECT)
            {
                "extending a final class"
            } else if flags.has(SEALED) && !self.syms.flags.contains_key(&c) {
                "extending a sealed class of the library"
            } else {
                continue;
            };
            self.unsupported(rhs, what);
        }
    }

    /// Checks that object class `cls` with template `rhs` is not defined in a universal trait,
    /// values between them skipped (dotty's `skipWeakOwner`).
    pub fn check_module_owner(&mut self, cls: SymbolId, rhs: NodeId) {
        if !self.u.symbol(cls).flags.has(OBJECT) {
            return;
        }
        let mut owner = self.u.symbol(cls).owner;
        while let Some(o) = owner {
            let sym = self.u.symbol(o);
            if sym.kind != Kind::Val || sym.flags.has(LAZY) {
                break;
            }
            owner = sym.owner;
        }
        let Some(o) = owner else { return };
        let object = self.u.builtins().object;
        if self.u.symbol(o).flags.has(TRAIT) && !self.u.derives_from(o, object) {
            self.unsupported_at(self.ast.span(rhs), "object in a universal trait");
        }
    }
}
