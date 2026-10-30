// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Skolemized prefixes of members selected on unstable qualifiers (dotty's `QualSkolemType`).

use super::Typer;
use super::approx::tycon_variances;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// `p`, or a fresh skolem of it if `p` is not stable and the type of term `s` as seen from
    /// it would put `p` for a `this` where it occurs other than covariantly (dotty's
    /// `maybeSkolemizePrefix` and `asSeenFrom` of a `QualSkolemType`).
    pub fn qual_skolem(&mut self, p: TypeId, s: SymbolId) -> TypeId {
        let unstable = match *self.u.types().get(p) {
            Type::Sym(_, c) => self.u.is_type_symbol(c) || !super::stability::is_stable_path(self.u, p),
            Type::TermRef(pre, ref name) => {
                let name = name.clone();
                let info = self.u.term_refinement_named(pre, &name);
                info.is_some_and(|i| matches!(self.u.types().get(i), Type::ByName(_) | Type::Method { .. }))
            }
            Type::Applied(..) | Type::Refined(..) | Type::And(..) | Type::Or(..) => true,
            _ => false,
        };
        let owner = self.u.symbol(s).owner;
        let Some(owner) = owner
            .filter(|&o| unstable && self.u.symbol(s).kind.is_term() && self.u.symbol(o).kind == Kind::Class)
        else {
            return p;
        };
        let info = self.u.info(s);
        match approximated(self.u, info, 1, (p, owner)) {
            true => self.u.fresh_skolem(p),
            false => p,
        }
    }
}

/// Whether seeing `t` at variance `v` from unstable `p` approximates a `this` of class `owner`.
fn approximated(u: &mut Universe, t: TypeId, v: i8, (p, owner): (TypeId, SymbolId)) -> bool {
    let m = |u: &mut Universe, t, v| approximated(u, t, v, (p, owner));
    match u.types().get(t).clone() {
        Type::This(_) => v <= 0 && is_owner_this(u, t, owner),
        Type::Sym(_, s) if u.is_static_symbol(s) => false,
        Type::Sym(Some(pre), s) if v <= 0 && is_owner_this(u, pre, owner) => not_forwarded(u, p, s),
        Type::Sym(Some(pre), _) => m(u, pre, v.max(0)),
        Type::Applied(tycon, args) => {
            let variances = tycon_variances(u, tycon, args.len());
            m(u, tycon, v)
                || args.iter().zip(variances).any(|(&a, pv)| {
                    let wildcard = matches!(u.types().get(a), Type::Bounds(..) | Type::Wildcard(_));
                    m(u, a, if wildcard { v } else { v * pv })
                })
        }
        Type::Alias(x) => m(u, x, 0),
        Type::Bounds(lo, hi) => m(u, lo, -v) || m(u, hi, v),
        Type::Method { params, result, .. }
        | Type::Poly { params, result, .. }
        | Type::Lambda { params, result, .. } => params.iter().any(|&(_, x)| m(u, x, -v)) || m(u, result, v),
        Type::And(x, y) | Type::Or(x, y, _) | Type::Refined(x, _, y) => m(u, x, v) || m(u, y, v),
        Type::ByName(x) | Type::Annotated(x, _) | Type::Rec(_, x) | Type::Skolem(_, x) => m(u, x, v),
        Type::Sym(None, _) | Type::Constant(..) | Type::Str(_) | Type::Param(..) => false,
        _ => {
            let mut found = false;
            u.map_type(t, &mut |u, x| {
                found |= is_owner_this(u, x, owner);
                None
            });
            found
        }
    }
}

/// Whether `t` is the `this` of class `owner` or of a class deriving from it.
fn is_owner_this(u: &mut Universe, t: TypeId, owner: SymbolId) -> bool {
    let Type::This(c) = *u.types().get(t) else { return false };
    u.class_symbol(c).is_some_and(|c| u.derives_from(c, owner))
}

/// Whether `this.s` seen from unstable `p` is a range rather than a type all values of `p`
/// agree on: an alias, a singleton, or the argument `p` gives a class type parameter.
fn not_forwarded(u: &mut Universe, p: TypeId, s: SymbolId) -> bool {
    if u.symbol(s).kind == Kind::TypeParam {
        let Some(class) = u.symbol(s).owner else { return true };
        let Some(index) = u.type_params(class).iter().position(|&q| q == s) else { return true };
        let arg =
            u.base_type(p, class).and_then(|b| u.applied(b)).and_then(|(_, args)| args.get(index).copied());
        return arg.is_none_or(|a| matches!(u.types().get(a), Type::Bounds(..)));
    }
    let info = u.info(s);
    !matches!(
        u.types().get(info),
        Type::Alias(_) | Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Skolem(..)
    ) && !matches!(u.types().get(info), Type::Sym(_, x) if u.symbol(*x).kind.is_term())
}
