//! Type bindings of patterns: the types a scrutinee's type gives the type variables of a pattern
//! type it conforms to (dotty's GADT approximation of quote pattern and inline match type
//! binds); the runtime of `QuoteMatching.ExprMatch.unapply` for a quote pattern of a typed term
//! hole (dotty's `QuoteMatcher.treeMatch`).

use crate::constraint::Constraint;
use crate::unpickle::QuotePattern;
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// The types the bindings of `pattern` stand for if an expression of type `scrutinee` matches
/// it, in their order.
pub fn quote_match(u: &mut Universe, scrutinee: TypeId, pattern: &QuotePattern) -> Option<Vec<TypeId>> {
    bound_types(u, scrutinee, pattern.hole, &pattern.bindings)
}

/// The types bindings `binds` stand for if `scrutinee` conforms to `pattern`, which refers to
/// them; each its lower bound, its upper one if approximated from above, unless instantiated.
pub fn bound_types(
    u: &mut Universe,
    scrutinee: TypeId,
    pattern: TypeId,
    binds: &[(SymbolId, bool)],
) -> Option<Vec<TypeId>> {
    let mut c = Constraint::default();
    let vars: Vec<(SymbolId, VarId, TypeId)> = binds
        .iter()
        .map(|&(s, _)| {
            let v = c.fresh(u);
            let Type::Var(id) = *u.types().get(v) else { unreachable!("a type variable") };
            (s, id, v)
        })
        .collect();
    let bound = |u: &mut Universe, t: TypeId| {
        u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => vars.iter().find(|&&(x, _, _)| x == s).map(|&(_, _, v)| v),
            _ => None,
        })
    };
    let mut bounds = Vec::new();
    for &(s, id, _) in &vars {
        let info = u.info(s);
        if let Type::Bounds(lo, hi) = *u.types().get(info) {
            bounds.push((id, bound(u, lo), bound(u, hi)));
        }
    }
    let pattern = bound(u, pattern);
    for (id, lo, hi) in bounds {
        let ok = (lo == NOTHING || c.add_lower(u, id, lo)) && (hi == ANY || c.add_upper(u, id, hi));
        if !ok {
            return None;
        }
    }
    if !u.is_subtype_in(scrutinee, pattern, &mut c) {
        return None;
    }
    let approximated = vars.iter().zip(binds).map(|(&(_, id, _), &(_, from_above))| {
        c.instance(id).unwrap_or_else(|| {
            let (lo, hi) = c.bounds(id);
            if from_above || lo == NOTHING { hi } else { lo }
        })
    });
    Some(approximated.collect())
}
