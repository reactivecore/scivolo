// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result type of a constructor, refined by its tracked parameters (dotty's
//! `effectiveResultType`), and the parameters dotty makes tracked.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::{Attrs, List, Tree};
use syntax::config::Features;
use tasty::tags::TRACKED;
use tasty::{Binder, Kind, ParamList, SymbolId, Type, TypeId};

/// The parameter clauses of a method type, by binder.
pub type Clauses = Vec<(Binder, ParamList)>;

impl Typer<'_> {
    /// Makes tracked, under `experimental.modularity`, the term parameters of method `sym` among
    /// `clauses` whose types have abstract type members: context bound evidence, as `[S: Coll]`
    /// with `type Pos` in `Coll`, and parameters with a public accessor, in a method of a class or
    /// nested in a given method. Their accessors become public and tracked; a parameter with a
    /// mutable accessor stays (dotty's `addTrackedIfNeeded` and `needsTracked`).
    pub fn add_tracked_if_needed(&mut self, sym: SymbolId, clauses: &[Vec<SymbolId>], ctx: Ctx) {
        let Some(owner) = self.u.symbol(sym).owner else { return };
        let given_method = f::GIVEN.or(f::METHOD);
        let in_given = self.syms.flags.get(&owner).is_some_and(|fl| fl.is_all_of(given_method));
        let in_class = self.u.is_class(owner);
        if !in_class && !in_given
            || !self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx)
        {
            return;
        }
        for &p in clauses.iter().flatten() {
            let Some(&tree) = self.syms.param_trees.get(&p) else { continue };
            if self.u.is_type_symbol(p) || self.flags_of(p).is(f::TRACKED) {
                continue;
            }
            let accessors = self.param_accessors(owner, in_class, p);
            let evidence = self.ast.attrs(tree).contains(Attrs::EVIDENCE);
            if accessors.iter().any(|&a| self.flags_of(a).is(f::MUTABLE))
                || !evidence && accessors.iter().all(|&a| self.flags_of(a).is_one_of(f::PRIVATE_LOCAL))
            {
                continue;
            }
            let info = self.method_param_type(p);
            if !self.has_abstract_type_members(info) {
                continue;
            }
            self.syms.set_flags(self.u, p, self.flags_of(p).or(f::TRACKED));
            for a in accessors {
                let flags = self.flags_of(a).and_not(f::PRIVATE_LOCAL).or(f::TRACKED);
                self.syms.set_flags(self.u, a, flags);
            }
        }
    }

    /// Result type `result` of method `sym`, refined by `{ val p: p.type }` for each tracked
    /// parameter `p` among `clauses` if `sym` is a given method under `experimental.modularity`
    /// (dotty's `wrapRefinedMethType`).
    pub fn given_result_refined(
        &mut self,
        sym: SymbolId,
        clauses: &[Vec<SymbolId>],
        result: TypeId,
        ctx: Ctx,
    ) -> TypeId {
        if !self.flags_of(sym).is_all_of(f::GIVEN.or(f::METHOD))
            || !self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx)
        {
            return result;
        }
        clauses.iter().flatten().fold(result, |r, &p| {
            if !self.flags_of(p).is(f::TRACKED) {
                return r;
            }
            let name = self.u.interned_name(p);
            let param = self.u.intern_type(Type::Sym(None, p));
            self.u.intern_type(Type::Refined(r, name, param))
        })
    }

    /// The dotty flags of script symbol `s`.
    fn flags_of(&self, s: SymbolId) -> f::FlagSet {
        self.syms.flags.get(&s).copied().unwrap_or(f::EMPTY)
    }

    /// The parameter accessors of class `owner` named as `p`; outside a class, `p` itself (dotty's
    /// `maybeParamAccessors`).
    fn param_accessors(&mut self, owner: SymbolId, in_class: bool, p: SymbolId) -> Vec<SymbolId> {
        if !in_class {
            return vec![p];
        }
        let name = self.u.symbol(p).name.clone();
        let mut accessors = self.u.decls_named(owner, &name);
        accessors.retain(|&a| self.flags_of(a).is(f::PARAM_ACCESSOR));
        accessors
    }

    /// Whether `qual` is a term parameter of a method with type parameters whose type refines
    /// type member `name` as an alias, as `tc.Self` in `def f[T: TC as tc](y: tc.Self)`, which
    /// dotty reduces to the alias in the parameter types of a later clause when the method is
    /// type-applied.
    pub fn is_aliased_param_member(&mut self, qual: TNodeId, name: &str) -> bool {
        let Type::Sym(_, param) = *self.u.ty(self.tpd.tpe(qual)) else { return false };
        let method = self.u.symbol(param).owner;
        let tree = method.and_then(|m| self.syms.def_trees.get(&m).copied());
        let Some(&Tree::DefDef { paramss, .. }) = tree.map(|t| self.ast.tree(t)) else { return false };
        let clauses = self.ast.clause_items(paramss);
        let first = |&c: &List| self.ast.items(c).first().map(|&p| self.ast.tree(p));
        if self.u.symbol(param).kind != Kind::Param
            || !clauses.iter().any(|c| matches!(first(c), Some(Tree::TypeDef { .. })))
        {
            return false;
        }
        let mut t = self.u.info(param);
        while let Type::Refined(parent, refined, info) = *self.u.ty(t) {
            if self.u.name(refined) == name && matches!(self.u.ty(info), Type::Alias(_)) {
                return true;
            }
            t = parent;
        }
        false
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
