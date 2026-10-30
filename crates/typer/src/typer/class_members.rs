// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The members of a class, found among its base classes.

use super::Typer;
use crate::symbols::Completion;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members named `name` of class `c`, its base classes' included: the first type member,
    /// or the term members, of each overriding pair the one dotty's `preferSym` prefers.
    pub fn class_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        let classes = self.completed_linearization(c);
        if name == "<init>" {
            return self.u.decl(c, name).unwrap_or_default();
        }
        self.members_of_bases(c, &classes, name, true)
    }

    /// The members named `name` of type `t`'s class, or its intersected classes', that are not
    /// private (dotty's `nonPrivateMember`).
    pub fn non_private_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        let mut widened = self.u.widen(t);
        if matches!(self.u.types().get(widened), Type::Or(..)) {
            widened = self.u.join(widened).unwrap_or(widened);
        }
        if let Type::And(a, b) = *self.u.types().get(widened) {
            let mut out = self.non_private_members(a, name);
            for m in self.non_private_members(b, name) {
                if !out.contains(&m) {
                    out.push(m);
                }
            }
            return out;
        }
        let Some(c) = self.u.class_symbol(widened) else { return Vec::new() };
        let classes = self.completed_linearization(c);
        self.members_of_bases(c, &classes, name, false)
    }

    /// The members named `name` that `super` selects in class `c`: those of its base classes but
    /// `c`.
    pub fn super_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        let classes = self.completed_linearization(c);
        self.members_of_bases(c, &classes[1..], name, true)
    }

    /// The linearization of class `c`, `c` and its base classes without TASTy completed.
    fn completed_linearization(&mut self, c: SymbolId) -> Vec<SymbolId> {
        self.complete(c);
        let classes = self.u.linearization(c);
        for &base in classes.iter().skip(1) {
            if self.u.symbol(base).tree.is_none() {
                self.complete(base);
            }
        }
        classes
    }

    /// The members named `name` of `classes`, base classes of `c` in linearization order, `c`'s
    /// private ones if `own_private`, those compared completed unless being completed.
    fn members_of_bases(
        &mut self,
        c: SymbolId,
        classes: &[SymbolId],
        name: &str,
        own_private: bool,
    ) -> Vec<SymbolId> {
        let site = crate::defn::type_ref(self.u, c);
        let mut out: Vec<SymbolId> = Vec::new();
        for &base in classes {
            let decls: Vec<SymbolId> = self.u.decl(base, name).unwrap_or_default();
            let decls: Vec<SymbolId> = decls
                .into_iter()
                .filter(|&m| base == c && own_private || !self.u.symbol(m).flags.has(tasty::tags::PRIVATE))
                .collect();
            for m in decls {
                if self.u.is_type_symbol(m) {
                    if !out.iter().any(|&o| self.u.is_type_symbol(o)) {
                        out.push(m);
                    }
                    continue;
                }
                let mut overridden = None;
                for (k, o) in out.clone().into_iter().enumerate() {
                    let overload = self.u.symbol(o).owner == Some(base)
                        && self.u.erased_method(o).0 != self.u.erased_method(m).0;
                    if self.u.is_type_symbol(o) || overload {
                        continue;
                    }
                    for s in [o, m] {
                        if matches!(self.syms.completions.get(&s), Some(Completion::Pending { .. })) {
                            self.complete(s);
                        }
                    }
                    if !self.signatures_differ(o, m) && self.same_member(site, o, m) {
                        overridden = Some(k);
                        break;
                    }
                }
                match overridden {
                    Some(k) if self.concrete_replaces(c, out[k], m) => out[k] = m,
                    Some(_) => {}
                    None => out.push(m),
                }
            }
        }
        out
    }
}
