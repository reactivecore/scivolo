// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type parameters a case's pattern may narrow and their current bounds.

use super::Typer;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe};

/// The type `t` refers to plainly or seen from `this` of its owner.
pub fn plain_type_ref(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    match *u.types().get(t) {
        Type::Sym(None, s) => Some(s),
        Type::Sym(Some(p), s) => match *u.types().get(p) {
            Type::This(c) => (u.class_symbol(c) == u.symbol(s).owner).then_some(s),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `t` has a part applying a class `scala.TupleN`, which dotty's GADT bounds hold as
/// `*:` pairs.
pub fn has_tuple_class(u: &mut Universe, t: TypeId) -> bool {
    let mut parts = Vec::new();
    u.map_type(t, &mut |_, p| {
        parts.push(p);
        None
    });
    parts.into_iter().any(|p| {
        u.class_symbol(p).is_some_and(|c| {
            let name = u.full_name(c);
            name.strip_prefix("scala.Tuple").is_some_and(|n| n.parse::<u8>().is_ok())
        })
    })
}

impl Typer<'_> {
    /// Adds pattern-bound types `syms` to the GADT-constrained types of the case being typed.
    pub fn gadt_bind(&mut self, syms: &[SymbolId]) {
        if self.gadt.cases > 0 {
            self.gadt.bound.extend(syms);
        }
    }

    /// Whether `s` is a GADT-constrained type: a type parameter of a method enclosing `ctx`'s
    /// owner or of a class enclosing one (dotty's `addToConstraint` in `typedDefDef`), or a
    /// pattern-bound type of a case being typed.
    pub fn is_gadt_symbol(&mut self, s: SymbolId, ctx: Ctx) -> bool {
        if self.gadt.bound.contains(&s) {
            return true;
        }
        let Some(of) = self.u.symbol(s).owner.filter(|_| self.u.symbol(s).kind == Kind::TypeParam) else {
            return false;
        };
        let (mut owner, mut in_method) = (Some(ctx.owner), false);
        while let Some(o) = owner {
            match self.u.symbol(o).kind {
                Kind::Def => in_method = true,
                Kind::Class if in_method => {}
                _ => {
                    owner = self.u.symbol(o).owner;
                    continue;
                }
            }
            if o == of {
                return true;
            }
            owner = self.u.symbol(o).owner;
        }
        false
    }

    /// The type parameters in `t` a pattern may narrow; none if `t` has a part outside the
    /// subset: a refinement, a singleton, a class's higher-kinded parameter or an abstract type
    /// constructor applied.
    pub fn gadt_symbols(&mut self, t: TypeId, ctx: Ctx) -> Vec<SymbolId> {
        if self.gadt.cases == 0 {
            return Vec::new();
        }
        let mut params = Vec::new();
        if !self.first_order_params(t, &mut params) {
            return Vec::new();
        }
        params.into_iter().filter(|&s| self.is_gadt_symbol(s, ctx)).collect()
    }

    /// Adds the type parameters `t` refers to outside prefixes to `params`; false if `t` has a
    /// refinement, a singleton, a class's higher-kinded type parameter or an abstract type
    /// constructor applied.
    fn first_order_params(&mut self, t: TypeId, params: &mut Vec<SymbolId>) -> bool {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::TypeParam || self.gadt.bound.contains(&s) => {
                let info = self.u.info(s);
                let Type::Bounds(_, hi) = *self.u.types().get(info) else { return false };
                let of_class = self.u.symbol(s).owner.is_some_and(|o| self.u.is_class(o));
                if of_class && matches!(self.u.types().get(hi), Type::Lambda { .. }) {
                    return false;
                }
                if !params.contains(&s) {
                    params.push(s);
                }
                true
            }
            Type::Sym(_, s) => !self.u.symbol(s).kind.is_term(),
            Type::Applied(tycon, args) => {
                self.u.class_symbol(tycon).is_some_and(|c| self.u.is_class(c))
                    && args.iter().all(|&a| self.first_order_params(a, params))
            }
            Type::And(a, b) | Type::Or(a, b, _) | Type::Bounds(a, b) => {
                self.first_order_params(a, params) && self.first_order_params(b, params)
            }
            Type::Annotated(a, _) | Type::Alias(a) => self.first_order_params(a, params),
            Type::Lambda { result, .. } => self.first_order_params(result, params),
            Type::TypeRef(..) | Type::This(_) | Type::Package(_) | Type::Param(..) => true,
            _ => false,
        }
    }

    /// A reference to type `s`, a class's type parameter seen from `this`.
    pub fn gadt_ref(&mut self, s: SymbolId) -> TypeId {
        let class = self
            .u
            .symbol(s)
            .owner
            .filter(|&o| self.u.symbol(s).kind == Kind::TypeParam && self.u.is_class(o));
        let prefix = class.map(|c| {
            let class = self.u.intern_type(Type::Sym(None, c));
            self.u.intern_type(Type::This(class))
        });
        self.u.intern_type(Type::Sym(prefix, s))
    }

    /// The bounds of type `s`: its GADT bounds, else its declared ones.
    pub fn current_bounds(&mut self, s: SymbolId) -> Option<(TypeId, TypeId)> {
        if let Some(b) = self.u.gadt_bounds(s) {
            return Some(b);
        }
        let info = self.u.info(s);
        match *self.u.types().get(info) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            _ => None,
        }
    }

    /// The bounds of the GADT variables `vars`, zonked.
    pub fn gadt_var_bounds(&mut self, vars: &[(SymbolId, TypeId)]) -> Vec<TypeId> {
        let mut bounds = Vec::new();
        for &(_, v) in vars {
            let Type::Var(id) = *self.u.types().get(v) else { continue };
            let (lo, hi) = self.constraint.bounds(id);
            bounds.extend([self.zonk(lo), self.zonk(hi)]);
        }
        bounds
    }

    /// Whether `t` refers to type `part`.
    pub fn refers_to_type(&mut self, t: TypeId, part: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |_, p| {
            found |= p == part;
            None
        });
        found
    }
}
