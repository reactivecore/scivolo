//! The runtime of `QuoteMatching.ExprMatch.unapply` for a quote pattern of a typed term hole
//! (dotty's `QuoteMatcher.treeMatch`): the scrutinee's type compared with the hole's, the
//! pattern's type bindings variables of the comparison, approximated after it.

use crate::constraint::Constraint;
use crate::unpickle::QuotePattern;
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// The types the bindings of `pattern` stand for if an expression of type `scrutinee` matches
/// it, in their order.
pub fn quote_match(u: &mut Universe, scrutinee: TypeId, pattern: &QuotePattern) -> Option<Vec<TypeId>> {
    let mut c = Constraint::default();
    let vars: Vec<(SymbolId, VarId, TypeId)> = pattern
        .bindings
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
    let hole = bound(u, pattern.hole);
    for (id, lo, hi) in bounds {
        let ok = (lo == NOTHING || c.add_lower(u, id, lo)) && (hi == ANY || c.add_upper(u, id, hi));
        if !ok {
            return None;
        }
    }
    if !u.is_subtype_in(scrutinee, hole, &mut c) {
        return None;
    }
    let approximated = vars.iter().zip(&pattern.bindings).map(|(&(_, id, _), &(_, from_above))| {
        c.instance(id).unwrap_or_else(|| {
            let (lo, hi) = c.bounds(id);
            if from_above { hi } else { lo }
        })
    });
    Some(approximated.collect())
}
