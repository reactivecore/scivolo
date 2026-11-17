// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result type of a constructor, refined by its tracked parameters (dotty's
//! `effectiveResultType`), and the parameters dotty makes tracked.

use super::Typer;
use crate::ctx::Ctx;
use names::nme;
use syntax::ast::Attrs;
use syntax::ast::flags as f;
use syntax::config::Features;
use tasty::tags::TRACKED;
use tasty::{Binder, Kind, ParamList, SymbolId, Type, TypeId};

/// The parameter clauses of a method type, by binder.
pub type Clauses = Vec<(Binder, ParamList)>;

impl Typer<'_> {
    /// Refuses the context bound evidence parameters of method `sym`, among `clauses`, that dotty
    /// makes tracked under `experimental.modularity` (dotty's `addTrackedIfNeeded` and
    /// `setTrackedConstrParam`): those of a constructor, of a method of a class or of a given
    /// method whose type has abstract type members, as `[S: Coll]` with `type Pos` in `Coll`.
    pub fn check_no_inferred_tracked(&mut self, sym: SymbolId, clauses: &[Vec<SymbolId>], ctx: Ctx) {
        let owner = self.u.symbol(sym).owner;
        let given = self.syms.flags.get(&sym).is_some_and(|fl| fl.is(f::GIVEN));
        if !given && !owner.is_some_and(|o| self.u.is_class(o)) {
            return;
        }
        for &p in clauses.iter().flatten() {
            let Some(&tree) = self.syms.param_trees.get(&p) else { continue };
            if self.u.is_type_symbol(p)
                || self.syms.flags.get(&p).is_some_and(|fl| fl.is(f::TRACKED))
                || !self.ast.attrs(tree).contains(Attrs::EVIDENCE) && !self.has_public_accessor(sym, p)
            {
                continue;
            }
            let info = self.method_param_type(p);
            if self.has_abstract_type_members(info)
                && self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx)
            {
                self.unsupported(tree, "context bound parameter dotty infers as tracked");
                return;
            }
        }
    }

    /// Whether parameter `p` of constructor `ctor` has a public, immutable accessor.
    fn has_public_accessor(&mut self, ctor: SymbolId, p: SymbolId) -> bool {
        let Some(class) = self.u.symbol(ctor).owner.filter(|_| self.u.symbol(ctor).name == nme::CONSTRUCTOR)
        else {
            return false;
        };
        let name = self.u.symbol(p).name.clone();
        let accessors = self.u.decls_named(class, &name);
        accessors.iter().any(|a| {
            self.syms
                .flags
                .get(a)
                .is_some_and(|fl| fl.is(f::PARAM_ACCESSOR) && !fl.is_one_of(f::PRIVATE_LOCAL.or(f::MUTABLE)))
        })
    }

    /// Whether `t` has an abstract type member its refinements do not alias.
    fn has_abstract_type_members(&mut self, mut t: TypeId) -> bool {
        let mut aliased = Vec::new();
        while let Type::Refined(parent, name, info) = *self.u.ty(t) {
            if matches!(self.u.ty(info), Type::Alias(_)) {
                aliased.push(self.u.name(name).to_owned());
            }
            t = parent;
        }
        let Some(class) = self.u.class_symbol(t) else { return false };
        for base in self.u.linearization(class) {
            for d in self.u.decls(base).unwrap_or_default() {
                if self.u.symbol(d).kind != Kind::Type {
                    continue;
                }
                self.complete(d);
                let info = self.u.info(d);
                let abstract_type = matches!(self.u.ty(info), Type::Bounds(..));
                if abstract_type && !aliased.contains(&self.u.symbol(d).name) {
                    return true;
                }
            }
        }
        false
    }

    /// Determines method type `t` of a constructor of `owner` with its final result, if `Unit`,
    /// replaced by `class`, refined by the tracked parameters of the clauses `outer` and of `t`.
    pub fn with_final_result(
        &mut self,
        owner: SymbolId,
        t: TypeId,
        class: TypeId,
        outer: &mut Clauses,
    ) -> TypeId {
        let node = *self.u.ty(t);
        let result = match node {
            Type::Method { binder, params, result, implicit, contextual } => {
                outer.push((binder, params));
                let result = self.with_final_result(owner, result, class, outer);
                return self.u.intern_type(Type::Method { binder, params, result, implicit, contextual });
            }
            _ if self.u.class_symbol(t) == Some(self.defn.unit) => class,
            _ if self.u.class_symbol(t) == Some(owner) && self.without_refinements(t) == t => t,
            _ => return t,
        };
        outer.iter().fold(result, |r, &(binder, params)| self.add_param_refinements(owner, r, binder, params))
    }

    /// Refines `class`, the type a constructor of class `owner` constructs, by `{ val p: p.type }`
    /// for each tracked parameter `p` of the clause `params` of the method type `binder` (dotty's
    /// `addParamRefinements`).
    fn add_param_refinements(
        &mut self,
        owner: SymbolId,
        class: TypeId,
        binder: Binder,
        params: ParamList,
    ) -> TypeId {
        if !self.u.symbol(owner).flags.has(TRACKED) {
            return class;
        }
        let names: Vec<_> = self.u.params_of(params).iter().map(|&(n, _)| n).collect();
        names.into_iter().enumerate().fold(class, |class, (i, name)| {
            let decls = self.u.decl(owner, name).unwrap_or_default();
            if !decls.iter().any(|&d| self.u.symbol(d).flags.has(TRACKED)) {
                return class;
            }
            let param = self.u.intern_type(Type::Param(binder, i as u32));
            self.u.intern_type(Type::Refined(class, name, param))
        })
    }

    /// `t` without the refinements around its class type.
    pub fn without_refinements(&self, mut t: TypeId) -> TypeId {
        while let Type::Refined(parent, ..) = *self.u.ty(t) {
            t = parent;
        }
        t
    }
}
