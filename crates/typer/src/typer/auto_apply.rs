// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Methods with an empty parameter list called without one (dotty's `isAutoApplied` in
//! `adaptNoArgsUnappliedMethod`), and parameterless ones called with one.

use super::Typer;
use crate::tpd::TNodeId;
use tasty::{SymbolId, Type};

impl Typer<'_> {
    /// Whether reference `tree` to a method with an empty parameter list may be called without
    /// one (dotty's `matchNullaryLoosely`): a member of `Any`, of a `java.*` or `javax.*` class or
    /// of the Scala 2 standard library, or one overriding such a member; warns about a member of
    /// the Scala 2 standard library that overrides no member of `Any` or a Java class
    /// (`warnScala2`).
    pub fn is_auto_applicable(&mut self, tree: TNodeId) -> bool {
        let Some(s) = self.tpd.sym(self.tpd.meth_part(tree)) else { return false };
        let Some(owner) = self.u.symbol(s).owner else { return false };
        let overridden = self.overridden_members(s, owner);
        if !self.is_loose_member(s) && !overridden.iter().any(|&m| self.is_loose_member(m)) {
            return false;
        }
        let any = self.u.builtins().any;
        let scala2 = std::iter::once(owner)
            .chain(overridden.iter().filter_map(|&m| self.u.symbol(m).owner))
            .any(|o| self.u.is_scala2(o));
        let java = overridden.iter().any(|&m| self.u.symbol(m).owner == Some(any) || self.is_java_member(m));
        if scala2 && !java {
            let span = self.tpd.span(tree);
            let msg = format!("method {} must be called with () argument", self.u.symbol(s).name);
            self.diags.warning_at(span.start(), span.end(), span.point(), msg);
        }
        true
    }

    /// Whether reference `tree` applied to `()` drops the empty arguments: it overrides a method
    /// with an empty parameter list (the 0th strategy of dotty's `tryInsertApplyOrImplicit`).
    pub fn drops_empty_args(&mut self, tree: TNodeId) -> bool {
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. } | Type::Error) {
            return false;
        }
        let Some(s) = self.tpd.sym(tree) else { return false };
        let Some(owner) = self.u.symbol(s).owner else { return false };
        self.overridden_members(s, owner).into_iter().any(|m| {
            let info = self.u.info(m);
            matches!(self.u.types().get(info), Type::Method { params, .. } if params.is_empty())
        })
    }

    /// The members of the base classes of `owner` that member `s` overrides.
    fn overridden_members(&mut self, s: SymbolId, owner: SymbolId) -> Vec<SymbolId> {
        let name = self.u.symbol(s).name.clone();
        let site = crate::defn::type_ref(self.u, owner);
        let mut out = Vec::new();
        for base in self.u.linearization(owner).into_iter().skip(1) {
            for m in self.u.decl(base, &name).unwrap_or_default() {
                let private = self.u.symbol(m).flags.has(tasty::tags::PRIVATE);
                if !private && self.same_member(site, m, s) {
                    out.push(m);
                }
            }
        }
        out
    }

    /// Whether `s` is a member of `Any`, of a `java.*` or `javax.*` class, or of a class of the
    /// Scala 2 standard library.
    fn is_loose_member(&mut self, s: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(s).owner else { return false };
        owner == self.u.builtins().any || self.u.is_scala2(owner) || self.is_java_member(s)
    }
}
