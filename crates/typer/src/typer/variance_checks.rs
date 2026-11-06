// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Variance checks of the definitions of classes (dotty's `VarianceChecker` as `PostTyper` runs
//! it).

use super::Typer;
use super::variance_types::{BIVARIANT, Checked, Misplaced, flip, is_unchecked_variance};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Reports each definition of a class in unit `root` whose type has a type parameter where
    /// its variance does not allow it, and each type lambda with variances its body breaks.
    pub fn check_variances(&mut self, root: TNodeId) {
        let mut errors = Vec::new();
        self.variances_in(root, &mut errors);
        for (span, msg) in errors {
            self.error_at(span, msg);
        }
    }

    fn variances_in(&mut self, t: TNodeId, errors: &mut Vec<(Span, String)>) {
        if self.tpd.is_type_tree(t) {
            return;
        }
        if let T::TypeDef { rhs, .. } = *self.tpd.tree(t) {
            match *self.tpd.tree(rhs) {
                T::Template { constr, body, .. } => {
                    self.check_def_variance(t, errors);
                    let body = self.tpd.items(body).to_vec();
                    for m in std::iter::once(constr).chain(body) {
                        if !self.is_class_def(m) {
                            self.check_def_variance(m, errors);
                        }
                    }
                }
                _ => self.check_lambda_variances(t, errors),
            }
        }
        for c in self.tpd.traversed(t) {
            self.variances_in(c, errors);
        }
    }

    fn is_class_def(&self, t: TNodeId) -> bool {
        matches!(*self.tpd.tree(t), T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }))
    }

    pub fn flags_of(&self, s: SymbolId) -> FlagSet {
        self.syms.flags.get(&s).copied().unwrap_or_default()
    }

    /// Checks definition `t` and, of a method, its parameters (dotty's `Traverser.traverse`).
    fn check_def_variance(&mut self, t: TNodeId, errors: &mut Vec<(Span, String)>) {
        let Some(sym) = self.tpd.sym(t) else { return };
        let flags = self.flags_of(sym);
        let owner = self.u.symbol(sym).owner;
        let skip = self.u.symbol(sym).name.starts_with("inline$")
            || flags.is_all_of(f::LOCAL_PARAM_ACCESSOR)
            || self.u.symbol(sym).kind == Kind::TypeParam && owner.is_some_and(|o| self.u.is_class(o));
        if skip {
            return;
        }
        match *self.tpd.tree(t) {
            T::TypeDef { .. } | T::ValDef { .. } => self.check_sym_variance(sym, t, errors),
            T::DefDef { paramss, .. } => {
                self.check_sym_variance(sym, t, errors);
                let clauses = self.tpd.clause_items(paramss).to_vec();
                let params: Vec<TNodeId> =
                    clauses.into_iter().flat_map(|l| self.tpd.items(l).to_vec()).collect();
                params.into_iter().for_each(|p| self.check_def_variance(p, errors));
            }
            _ => {}
        }
    }

    /// Checks the type of `sym`, defined by `t` (dotty's `checkVariance`).
    fn check_sym_variance(&mut self, sym: SymbolId, t: TNodeId, errors: &mut Vec<(Span, String)>) {
        let flags = self.flags_of(sym);
        let local = flags.is_all_of(f::PRIVATE_LOCAL)
            || flags.is(f::PRIVATE) && !self.assigned_non_locally.contains(&sym);
        let kind = self.u.symbol(sym).kind;
        let variance = if flags.is(f::MUTABLE) && kind != Kind::Def && !local { 0 } else { 1 };
        let checked = Checked::Definition(sym);
        let unchecked = if kind == Kind::Class { self.unchecked_parents(t) } else { Vec::new() };
        let found = match kind {
            Kind::Class => {
                let parents = self.u.parents(sym).into_iter().filter(|p| !unchecked.contains(p));
                parents.collect::<Vec<_>>().into_iter().find_map(|p| self.misplaced(checked, p, 1, 0))
            }
            _ => {
                let mut info = self.u.info(sym);
                while let Type::Method { result, .. } | Type::Poly { result, .. } = *self.u.types().get(info)
                {
                    info = result;
                }
                self.misplaced(checked, info, variance, 0)
            }
        };
        let Some(Misplaced::Class(tvar, required)) = found else { return };
        let msg = self.variance_message(sym, tvar, required, &unchecked);
        errors.push((self.tpd.span(t), msg));
    }

    /// The parent types of class definition `t` annotated `@uncheckedVariance`.
    fn unchecked_parents(&mut self, t: TNodeId) -> Vec<TypeId> {
        let T::TypeDef { rhs, .. } = *self.tpd.tree(t) else { return Vec::new() };
        let T::Template { parents, .. } = *self.tpd.tree(rhs) else { return Vec::new() };
        let mut out = Vec::new();
        for mut p in self.tpd.items(parents).to_vec() {
            while let T::Apply { fun: q, .. }
            | T::TypeApply { fun: q, .. }
            | T::Select { qualifier: q, .. }
            | T::New { tpt: q } = *self.tpd.tree(p)
            {
                p = q;
            }
            if let Type::Annotated(inner, annot) = *self.u.types().get(self.tpd.tpe(p))
                && is_unchecked_variance(self.u, annot)
            {
                out.push(self.u.dealias(inner));
            }
        }
        out
    }

    /// The variance of an occurrence of type parameter `tvar` in the type of `base`, seen from
    /// the definition of `tvar`: `v` changed along the owners of `base` up to the owner of `tvar`
    /// (dotty's `relativeVariance`).
    pub fn relative_variance(&mut self, tvar: SymbolId, mut base: SymbolId, mut v: u8) -> u8 {
        let towner = self.u.symbol(tvar).owner;
        loop {
            if Some(base) == towner {
                return v;
            }
            let Some(owner) = self.u.symbol(base).owner else { return BIVARIANT };
            let (kind, owner_kind) = (self.u.symbol(base).kind, self.u.symbol(owner).kind);
            let param = matches!(kind, Kind::Param | Kind::TypeParam);
            if param && owner_kind.is_term() && !self.flags_of(owner).is_all_of(f::PRIVATE_LOCAL) {
                let outer = self.u.symbol(owner).owner;
                let ctor = self.u.symbol(owner).name == "<init>";
                let outer = if ctor { outer.and_then(|o| self.u.symbol(o).owner) } else { outer };
                let Some(outer) = outer else { return BIVARIANT };
                (base, v) = (outer, flip(v));
            } else if owner_kind.is_term()
                || owner_kind == Kind::Package
                || self.flags_of(base).is_all_of(f::PRIVATE_LOCAL)
            {
                return BIVARIANT;
            } else if kind == Kind::Type
                && (self.flags_of(base).is(f::OPAQUE) || self.u.alias_of(base).is_some())
            {
                (base, v) = (owner, 0);
            } else {
                base = owner;
            }
        }
    }

    /// Records variable `var`, assigned through `lhs`, as assigned other than through its
    /// owner's `this` if it is private but not local (dotty's `AssignedNonLocally`).
    pub fn note_assignment(&mut self, var: SymbolId, lhs: TNodeId) {
        let flags = self.flags_of(var);
        if !flags.is(f::PRIVATE) || flags.is(f::LOCAL) {
            return;
        }
        let T::Select { qualifier, .. } = *self.tpd.tree(lhs) else { return };
        let prefix = self.tpd.tpe(qualifier);
        let Some(owner) = self.u.symbol(var).owner else { return };
        if !self.is_privileged_prefix(prefix, owner) {
            self.assigned_non_locally.insert(var);
        }
    }
}
