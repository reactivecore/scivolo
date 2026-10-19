//! Type bindings of patterns: the types a scrutinee's type gives the type variables of a pattern
//! type it conforms to (dotty's GADT approximation of quote pattern and inline match type
//! binds); the runtime of `QuoteMatching.ExprMatch.unapply` for a quote pattern of a typed term
//! hole (dotty's `QuoteMatcher.treeMatch`).

use crate::constraint::Constraint;
use crate::unpickle::QuotePattern;
use rustc_hash::FxHashMap;
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// The types the bindings of `pattern` stand for if an expression of type `scrutinee` matches
/// it, in their order.
pub fn quote_match(u: &mut Universe, scrutinee: TypeId, pattern: &QuotePattern) -> Option<Vec<TypeId>> {
    bound_types(u, scrutinee, pattern.hole, &pattern.bindings)
}

/// Each of `binds` with whether it occurs in `t` only contravariantly, so is approximated from
/// above (dotty's `InlineReducer.getTypeBindsMap`).
pub fn bind_variances(u: &mut Universe, t: TypeId, binds: &[SymbolId]) -> Vec<(SymbolId, bool)> {
    let mut found: FxHashMap<SymbolId, i8> = FxHashMap::default();
    collect_variances(u, t, 1, binds, &mut found, 0);
    binds.iter().map(|&s| (s, found.get(&s) == Some(&-1))).collect()
}

fn collect_variances(
    u: &mut Universe,
    t: TypeId,
    v: i8,
    binds: &[SymbolId],
    found: &mut FxHashMap<SymbolId, i8>,
    depth: u32,
) {
    if depth > 32 {
        return;
    }
    let mut go = |u: &mut Universe, t, v| collect_variances(u, t, v, binds, found, depth + 1);
    match u.types().get(t).clone() {
        Type::Sym(None, s) if binds.contains(&s) => {
            let entry = found.entry(s).or_insert(v);
            if *entry != v {
                *entry = 0;
            }
        }
        Type::Applied(tycon, args) => {
            let variances = match *u.types().get(tycon) {
                Type::Sym(_, c) => u.variances(c),
                _ => Vec::new(),
            };
            for (i, a) in args.into_iter().enumerate() {
                go(u, a, v * variances.get(i).copied().unwrap_or(0));
            }
            go(u, tycon, v);
        }
        Type::And(a, b) | Type::Or(a, b, _) => {
            go(u, a, v);
            go(u, b, v);
        }
        Type::Bounds(lo, hi) => {
            go(u, lo, -v);
            go(u, hi, v);
        }
        _ => {
            let mut parts = Vec::new();
            u.map_type(t, &mut |u, part| {
                if matches!(*u.types().get(part), Type::Sym(None, s) if binds.contains(&s)) {
                    parts.push(part);
                }
                None
            });
            for p in parts.into_iter().filter(|&p| p != t) {
                go(u, p, 0);
            }
        }
    }
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
            if from_above { hi } else { lo }
        })
    });
    Some(approximated.collect())
}
