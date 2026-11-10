// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Variance checks of the definitions of classes (dotty's `VarianceChecker` as `PostTyper` runs
//! it).

use super::Typer;
use super::variance_types::{BIVARIANT, Checked, Misplaced, VarianceOps, flip, is_unchecked_variance};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

/// One variance check of a typed unit: it finds each definition in a class whose type mentions a
/// type parameter of the class where its variance does not allow it, as `def f(a: A)` in
/// `class C[+A]`, and each type lambda whose body breaks the variances of its parameters, as
/// `type F[+X] = X => Int`.
pub struct VarianceCheck<'a> {
    pub tpd: &'a Tpd,
    /// The private variables assigned other than through their owner's `this`.
    pub assigned_non_locally: &'a FxHashSet<SymbolId>,
    /// The errors found, with dotty's messages.
    pub errors: Vec<(Span, String)>,
}

impl Typer<'_> {
    /// Reports each definition of a class in unit `root` whose type has a type parameter where
    /// its variance does not allow it, and each type lambda with variances its body breaks.
    pub fn check_variances(&mut self, root: TNodeId) {
        let mut ops = VarianceOps { u: self.u, syms: &self.syms, aliases: &mut self.alias_variances };
        let mut check = VarianceCheck::new(&self.tpd, &self.assigned_non_locally);
        check.variances_in(&mut ops, root);
        for (span, msg) in check.errors {
            self.error_at(span, msg);
        }
    }

    /// Records variable `var`, assigned through `lhs`, as assigned other than through its
    /// owner's `this` if it is private but not local (dotty's `AssignedNonLocally`).
    pub fn note_assignment(&mut self, var: SymbolId, lhs: TNodeId) {
        let flags = self.syms.flags_of(var);
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

impl<'a> VarianceCheck<'a> {
    /// Starts a check of the trees of `tpd`, in which the variables `assigned_non_locally` are
    /// assigned other than through their owner's `this`.
    pub fn new(tpd: &'a Tpd, assigned_non_locally: &'a FxHashSet<SymbolId>) -> Self {
        VarianceCheck { tpd, assigned_non_locally, errors: Vec::new() }
    }

    /// Checks the class and type definitions in tree `t` and its subtrees.
    pub fn variances_in(&mut self, ops: &mut VarianceOps, t: TNodeId) {
        if self.tpd.is_type_tree(t) {
            return;
        }
        let tpd = self.tpd;
        if let T::TypeDef { rhs, .. } = *tpd.tree(t) {
            match *tpd.tree(rhs) {
                T::Template { constr, body, .. } => {
                    self.check_def_variance(ops, t);
                    for &m in std::iter::once(&constr).chain(tpd.items(body)) {
                        if !self.is_class_def(m) {
                            self.check_def_variance(ops, m);
                        }
                    }
                }
                _ => self.check_lambda_variances(ops, t),
            }
        }
        for c in tpd.traversed(t) {
            self.variances_in(ops, c);
        }
    }

    fn is_class_def(&self, t: TNodeId) -> bool {
        matches!(*self.tpd.tree(t), T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }))
    }

    /// Checks definition `t` and, of a method, its parameters (dotty's `Traverser.traverse`).
    fn check_def_variance(&mut self, ops: &mut VarianceOps, t: TNodeId) {
        let Some(sym) = self.tpd.sym(t) else { return };
        let flags = ops.syms.flags_of(sym);
        let owner = ops.u.symbol(sym).owner;
        let skip = ops.u.symbol(sym).name.starts_with("inline$")
            || flags.is_all_of(f::LOCAL_PARAM_ACCESSOR)
            || ops.u.symbol(sym).kind == Kind::TypeParam && owner.is_some_and(|o| ops.u.is_class(o));
        if skip {
            return;
        }
        let tpd = self.tpd;
        match *tpd.tree(t) {
            T::TypeDef { .. } | T::ValDef { .. } => self.check_sym_variance(ops, sym, t),
            T::DefDef { paramss, .. } => {
                self.check_sym_variance(ops, sym, t);
                for &p in tpd.clause_items(paramss).iter().flat_map(|&l| tpd.items(l)) {
                    self.check_def_variance(ops, p);
                }
            }
            _ => {}
        }
    }

    /// Checks the type of `sym`, defined by `t` (dotty's `checkVariance`).
    fn check_sym_variance(&mut self, ops: &mut VarianceOps, sym: SymbolId, t: TNodeId) {
        let flags = ops.syms.flags_of(sym);
        let local = flags.is_all_of(f::PRIVATE_LOCAL)
            || flags.is(f::PRIVATE) && !self.assigned_non_locally.contains(&sym);
        let kind = ops.u.symbol(sym).kind;
        let variance = if flags.is(f::MUTABLE) && kind != Kind::Def && !local { 0 } else { 1 };
        let checked = Checked::Definition(sym);
        let unchecked = if kind == Kind::Class { self.unchecked_parents(ops, t) } else { Vec::new() };
        let found = match kind {
            Kind::Class => {
                let parents = ops.u.parents(sym).into_iter().filter(|p| !unchecked.contains(p));
                parents.collect::<Vec<_>>().into_iter().find_map(|p| ops.misplaced(checked, p, 1, 0))
            }
            _ => {
                let mut info = ops.u.info(sym);
                while let Type::Method { result, .. } | Type::Poly { result, .. } = *ops.u.types().get(info) {
                    info = result;
                }
                ops.misplaced(checked, info, variance, 0)
            }
        };
        let Some(Misplaced::Class(tvar, required)) = found else { return };
        let msg = ops.variance_message(sym, tvar, required, &unchecked);
        self.errors.push((self.tpd.span(t), msg));
    }

    /// The parent types of class definition `t` annotated `@uncheckedVariance`.
    fn unchecked_parents(&self, ops: &mut VarianceOps, t: TNodeId) -> Vec<TypeId> {
        let T::TypeDef { rhs, .. } = *self.tpd.tree(t) else { return Vec::new() };
        let T::Template { parents, .. } = *self.tpd.tree(rhs) else { return Vec::new() };
        let mut out = Vec::new();
        for &(mut p) in self.tpd.items(parents) {
            while let T::Apply { fun: q, .. }
            | T::TypeApply { fun: q, .. }
            | T::Select { qualifier: q, .. }
            | T::New { tpt: q } = *self.tpd.tree(p)
            {
                p = q;
            }
            if let Type::Annotated(inner, annot) = *ops.u.types().get(self.tpd.tpe(p))
                && is_unchecked_variance(ops.u, annot)
            {
                out.push(ops.u.dealias(inner));
            }
        }
        out
    }
}

impl VarianceOps<'_> {
    /// Determines the variance of an occurrence of type parameter `tvar` in the type of `base`,
    /// seen from the definition of `tvar`: `v` changed along the owners of `base` up to the owner
    /// of `tvar` (dotty's `relativeVariance`). A parameter of a method of class `C[+A]` is seen
    /// contravariantly; a private local member's type is not checked, so bivariant.
    pub fn relative_variance(&mut self, tvar: SymbolId, mut base: SymbolId, mut v: u8) -> u8 {
        let towner = self.u.symbol(tvar).owner;
        loop {
            if Some(base) == towner {
                return v;
            }
            let Some(owner) = self.u.symbol(base).owner else { return BIVARIANT };
            let (kind, owner_kind) = (self.u.symbol(base).kind, self.u.symbol(owner).kind);
            let param = matches!(kind, Kind::Param | Kind::TypeParam);
            if param && owner_kind.is_term() && !self.syms.flags_of(owner).is_all_of(f::PRIVATE_LOCAL) {
                let outer = self.u.symbol(owner).owner;
                let ctor = self.u.symbol(owner).name == "<init>";
                let outer = if ctor { outer.and_then(|o| self.u.symbol(o).owner) } else { outer };
                let Some(outer) = outer else { return BIVARIANT };
                (base, v) = (outer, flip(v));
            } else if owner_kind.is_term()
                || owner_kind == Kind::Package
                || self.syms.flags_of(base).is_all_of(f::PRIVATE_LOCAL)
            {
                return BIVARIANT;
            } else if kind == Kind::Type
                && (self.syms.flags_of(base).is(f::OPAQUE) || self.u.alias_of(base).is_some())
            {
                (base, v) = (owner, 0);
            } else {
                base = owner;
            }
        }
    }
}
