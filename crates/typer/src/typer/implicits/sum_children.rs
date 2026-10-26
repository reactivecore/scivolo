// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//! The children of a sealed class (dotty's `children`, `isGenericSum`), and their types as the
//! elements of its sum mirror.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::sealed;
use tasty::tags::{ABSTRACT, CONTRAVARIANT, ENUM, SEALED};
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The children of sealed class `s` in the order of their definitions: classes, and objects
    /// by their values; of an enum, its cases. Refused for a sealed class of the library.
    pub fn sealed_children(&mut self, s: SymbolId) -> Result<Vec<SymbolId>, &'static str> {
        let Some(&def) = self.syms.def_trees.get(&s) else {
            return Err("sum Mirror of a library's sealed class");
        };
        if self.u.symbol(s).flags.has(ENUM) {
            return Ok(sealed::enum_cases(self.u, s));
        }
        let file = self.file_of(self.ast.span(def));
        let defs: Vec<(SymbolId, syntax::ast::NodeId)> =
            self.syms.def_trees.iter().map(|(&k, &v)| (k, v)).collect();
        let mut classes = Vec::new();
        for (c, node) in defs {
            let span = self.ast.span(node);
            if self.u.symbol(c).kind == Kind::Class && self.file_of(span) == file {
                self.complete(c);
                classes.push((span.start(), c));
            }
        }
        Ok(sealed::direct_children(self.u, s, &classes))
    }

    /// Whether sealed class `s` at prefix `pre` has a sum mirror (dotty's `isGenericSum`), its
    /// children visible from `ctx` or through `pre` unless its companion is the mirror.
    pub fn is_generic_sum(
        &mut self,
        s: SymbolId,
        (companion, pre): (bool, Option<TypeId>),
        ctx: Ctx,
    ) -> Result<bool, &'static str> {
        let flags = self.u.symbol(s).flags;
        if !flags.has(SEALED) || !(flags.has(ABSTRACT) || self.u.is_trait(s)) {
            return Ok(false);
        }
        let children = self.sealed_children(s)?;
        if children.is_empty() {
            return Ok(false);
        }
        let scope = self.owner_scope(pre);
        for c in children {
            if self.u.symbol(c).name.starts_with("$anon") {
                return Ok(false);
            }
            let owner = self.u.symbol(c).owner.ok_or("sealed child without owner")?;
            if !sealed::visible_to_parent(self.u, s, owner) {
                return Ok(false);
            }
            if !companion && !self.is_visible_to_scope(owner, ctx.owner, &scope) {
                return Err("sum Mirror of a child visible through a prefix only");
            }
            if !self.u.is_class(c) {
                continue;
            }
            let fits = self.is_generic_product(c, ctx)
                || self.u.symbol(c).flags.has(SEALED) && self.is_generic_sum(c, (companion, pre), ctx)?;
            if !fits {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The type of child `c` of the sum mirrored as `target` (dotty's `solve`), at prefix
    /// `prefix` if given: an object's or value's singleton type, a class with its type parameters
    /// solved from `C[Xs] <: target`, contravariant ones minimized, the others maximized.
    pub fn child_type(
        &mut self,
        c: SymbolId,
        target: TypeId,
        prefix: Option<TypeId>,
    ) -> Result<TypeId, &'static str> {
        let raw = match prefix {
            Some(p) => {
                let s = match self.u.is_module_class(c) {
                    true => self.u.source_module(c).unwrap_or(c),
                    false => c,
                };
                let p = (!matches!(self.u.types().get(p), Type::NoPrefix)).then_some(p);
                self.u.intern_type(Type::Sym(p, s))
            }
            None => self.reachable_ref(c),
        };
        let params = match self.u.is_class(c) {
            true => self.u.type_params(c),
            false => Vec::new(),
        };
        if params.is_empty() {
            return Ok(raw);
        }
        let mark = self.constraint.mark();
        let vars: Vec<TypeId> = params.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let bounded = self.bound_child_vars(&params, &vars);
        let applied = self.u.intern_type(Type::Applied(raw, vars.clone()));
        let before = self.constraint.mark();
        if !self.conforms(applied, target) {
            self.constraint.rollback(before);
        }
        let solved = bounded && self.instantiate_child_vars(&params, &vars).is_ok();
        let t = self.zonk(applied);
        self.constraint.rollback(mark);
        match solved && self.is_fully_defined(t) {
            true => Ok(t),
            false => Err("sum Mirror of a child whose type parameters are not solved"),
        }
    }

    /// A reference to `s` reachable from outside its owners (dotty's `reachableTermRef`): through
    /// the paths of the objects that are not static it is nested in.
    fn reachable_ref(&mut self, s: SymbolId) -> TypeId {
        let owner = self.u.symbol(s).owner;
        match owner.filter(|&o| self.u.is_module_class(o) && !self.u.is_static_symbol(o)) {
            Some(o) => match self.u.source_module(o) {
                Some(module) => {
                    let prefix = self.reachable_ref(module);
                    self.u.intern_type(Type::Sym(Some(prefix), s))
                }
                None => crate::defn::type_ref(self.u, s),
            },
            None => crate::defn::type_ref(self.u, s),
        }
    }

    /// Constrains `vars` by the declared bounds of type parameters `params` (dotty's
    /// `constrained` of the primary constructor's type); `false` if they contradict.
    fn bound_child_vars(&mut self, params: &[SymbolId], vars: &[TypeId]) -> bool {
        let mut ok = true;
        for (&p, &v) in params.iter().zip(vars) {
            self.complete(p);
            let info = self.u.info(p);
            let Type::Bounds(lo, hi) = *self.u.types().get(info) else { continue };
            let subst = |u: &mut tasty::Universe, t: TypeId| {
                u.map_type(t, &mut |u, x| match *u.types().get(x) {
                    Type::Sym(_, s) => params.iter().position(|&q| q == s).map(|i| vars[i]),
                    _ => None,
                })
            };
            let (lo, hi) = (subst(self.u, lo), subst(self.u, hi));
            ok &= self.conforms(lo, v) && self.conforms(v, hi);
        }
        ok
    }

    fn instantiate_child_vars(&mut self, params: &[SymbolId], vars: &[TypeId]) -> Result<(), &'static str> {
        for (&p, &v) in params.iter().zip(vars) {
            let Type::Var(v) = *self.u.types().get(v) else { continue };
            if self.constraint.instance(v).is_none() {
                let below = self.u.symbol(p).flags.has(CONTRAVARIANT);
                self.instantiate_var(v, below)?;
            }
        }
        Ok(())
    }
}
