// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type parameters a case's pattern may narrow and their current bounds.

use super::gadt_cases::Gadt;
use super::narrowing::GadtVar;
use super::type_ops::TypeOps;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe};

/// Finds the type `t` refers to plainly, as `T`, or seen from `this` of its owner.
pub fn plain_type_ref(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let (prefix, s) = u.sym_ref(t)?;
    let Some(p) = prefix else { return Some(s) };
    let c = u.this_of(p)?;
    (u.class_symbol(c) == u.symbol(s).owner).then_some(s)
}

/// Makes a reference to type `s`, a class's type parameter seen from `this`.
pub fn gadt_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let class = u.symbol(s).owner.filter(|&o| u.symbol(s).kind == Kind::TypeParam && u.is_class(o));
    let prefix = class.map(|c| {
        let class = u.intern_type(Type::Sym(None, c));
        u.intern_type(Type::This(class))
    });
    u.intern_type(Type::Sym(prefix, s))
}

/// Gives the bounds of type `s`: those a case narrowed it to, else its declared ones.
pub fn current_bounds(u: &mut Universe, s: SymbolId) -> Option<(TypeId, TypeId)> {
    if let Some(b) = u.gadt_bounds(s) {
        return Some(b);
    }
    let info = u.info(s);
    u.bounds(info)
}

impl Gadt {
    /// Tells whether a case may narrow `s`: a type parameter of a method enclosing `ctx`'s owner,
    /// of a polymorphic function value's closure enclosing it, or of a class enclosing one
    /// (dotty's `addToConstraint` in `typedDefDef`), or a type bound by a pattern of a case being
    /// typed.
    pub fn is_symbol(&self, u: &mut Universe, s: SymbolId, ctx: Ctx) -> bool {
        if self.bound.contains(&s) {
            return true;
        }
        let Some(of) = u.symbol(s).owner.filter(|_| u.symbol(s).kind == Kind::TypeParam) else {
            return false;
        };
        let poly = self.poly_owners.contains(&of).then(|| u.symbol(of).owner);
        let (mut owner, mut in_method) = (Some(ctx.owner), false);
        while let Some(o) = owner {
            match u.symbol(o).kind {
                Kind::Def => in_method = true,
                Kind::Class if in_method => {}
                _ => {
                    owner = u.symbol(o).owner;
                    continue;
                }
            }
            if o == of || poly.is_some_and(|p| u.symbol(o).owner == p) {
                return true;
            }
            owner = u.symbol(o).owner;
        }
        false
    }

    /// Lists the type parameters in `t` a pattern may narrow; none if `t` has a part outside the
    /// subset: a refinement, a singleton, a class's higher-kinded parameter or an abstract type
    /// constructor applied.
    pub fn symbols(&self, u: &mut Universe, t: TypeId, ctx: Ctx) -> Vec<SymbolId> {
        if self.cases == 0 {
            return Vec::new();
        }
        let mut params = Vec::new();
        if !self.first_order_params(u, t, &mut params) {
            return Vec::new();
        }
        params.into_iter().filter(|&s| self.is_symbol(u, s, ctx)).collect()
    }

    /// Extends `syms` by the narrowable type parameters of pattern type `pat`, of its base type at
    /// class `base`, and those the bounds of each refer to, transitively (dotty's GADT constraint
    /// relating its symbols).
    pub fn related(
        &self,
        u: &mut Universe,
        mut syms: Vec<SymbolId>,
        (pat, base): (TypeId, Option<SymbolId>),
        ctx: Ctx,
    ) -> Vec<SymbolId> {
        let mut todo = self.symbols(u, pat, ctx);
        if let Some(b) = base.and_then(|c| u.base_type(pat, c)) {
            todo.extend(self.symbols(u, b, ctx));
        }
        let mut i = 0;
        while i < syms.len() || !todo.is_empty() {
            if let Some(s) = todo.pop() {
                if !syms.contains(&s) {
                    syms.push(s);
                }
                continue;
            }
            if let Some((lo, hi)) = current_bounds(u, syms[i]) {
                todo.extend(self.symbols(u, lo, ctx));
                todo.extend(self.symbols(u, hi, ctx));
            }
            i += 1;
        }
        syms
    }

    /// Adds the type parameters `t` refers to outside prefixes to `params`; false if `t` has a
    /// refinement, a singleton, a class's higher-kinded type parameter or an abstract type
    /// constructor applied.
    fn first_order_params(&self, u: &mut Universe, t: TypeId, params: &mut Vec<SymbolId>) -> bool {
        match *u.ty(t) {
            Type::Sym(_, s) if u.symbol(s).kind == Kind::TypeParam || self.bound.contains(&s) => {
                let info = u.info(s);
                let Some((_, hi)) = u.bounds(info) else { return false };
                let of_class = u.symbol(s).owner.is_some_and(|o| u.is_class(o));
                if of_class && u.lambda(hi).is_some() {
                    return false;
                }
                if !params.contains(&s) {
                    params.push(s);
                }
                true
            }
            Type::Sym(_, s) => !u.symbol(s).kind.is_term(),
            Type::Applied(tycon, args) => {
                u.class_symbol(tycon).is_some_and(|c| u.is_class(c))
                    && u.types_of(args).to_vec().into_iter().all(|a| self.first_order_params(u, a, params))
            }
            Type::And(a, b) | Type::Or(a, b, _) | Type::Bounds(a, b) => {
                self.first_order_params(u, a, params) && self.first_order_params(u, b, params)
            }
            Type::Annotated(a, _) | Type::Alias(a) => self.first_order_params(u, a, params),
            Type::Lambda { result, .. } => self.first_order_params(u, result, params),
            Type::TypeRef(..) | Type::This(_) | Type::Package(_) | Type::Param(..) => true,
            _ => false,
        }
    }
}

impl TypeOps<'_> {
    /// Lists the lower and upper bounds of the type variables standing for the type parameters
    /// an extractor pattern narrows, zonked.
    pub fn gadt_var_bounds(&mut self, vars: &[GadtVar]) -> Vec<TypeId> {
        let mut bounds = Vec::new();
        for &(_, v) in vars {
            let Some(id) = self.u.var(v) else { continue };
            let (lo, hi) = self.constraint.bounds(id);
            bounds.extend([self.zonk(lo), self.zonk(hi)]);
        }
        bounds
    }

    /// Tells whether `t` refers to a type variable.
    pub fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, p| {
            found |= u.var(p).is_some();
            None
        });
        found
    }

    /// Tells whether `t` refers to type `part`.
    pub fn refers_to_type(&mut self, t: TypeId, part: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |_, p| {
            found |= p == part;
            None
        });
        found
    }
}
