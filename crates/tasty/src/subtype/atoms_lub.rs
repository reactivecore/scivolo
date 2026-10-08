//! Least upper bounds of types by their atoms (dotty's `lub`, its step on atoms), and the shape
//! of the unions it builds.

use super::atoms::{members, subset};
use super::{Cmp, NoVars};
use crate::Universe;
use crate::types::{Type, TypeId};

impl Universe {
    /// The least upper bound of `a` and `b` by their atoms if both have known atoms and one
    /// contains the other or they are disjoint; `None` otherwise.
    pub fn atoms_lub(&mut self, a: TypeId, b: TypeId) -> Option<TypeId> {
        let mut vars = NoVars;
        let mut c = Cmp::new(self, &mut vars);
        let (r1, r2) = (c.atoms(a)?, c.atoms(b)?);
        if subset(&r1.hi, &r2.lo) {
            return Some(b);
        }
        if subset(&r2.hi, &r1.lo) {
            return Some(a);
        }
        let shared = members(&r1.hi, r2.hi.len());
        (!r2.hi.iter().any(shared)).then(|| union(self, a, b))
    }
}

/// `a | b`, balanced (dotty's `OrType.balanced`); `b` appended to `a` if `b` is no union, the
/// result a binary counter of complete subtrees.
fn union(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    let mut parts = Vec::new();
    or_parts(u, a, &mut parts);
    let left = parts.len();
    or_parts(u, b, &mut parts);
    match parts.len() - left {
        right if left.abs_diff(right) <= 1 => u.intern_type(Type::Or(a, b)),
        1 => appended(u, a, left, b),
        _ => balanced(u, &parts),
    }
}

fn or_parts(u: &Universe, t: TypeId, out: &mut Vec<TypeId>) {
    match *u.types().get(t) {
        Type::Or(x, y) => {
            or_parts(u, x, out);
            or_parts(u, y, out);
        }
        _ => out.push(t),
    }
}

fn balanced(u: &mut Universe, parts: &[TypeId]) -> TypeId {
    if let [t] = parts {
        return *t;
    }
    let (a, b) = parts.split_at(parts.len() / 2);
    let (a, b) = (balanced(u, a), balanced(u, b));
    u.intern_type(Type::Or(a, b))
}

/// `tree`, a union of `k` parts built by `appended`, with `part` added.
fn appended(u: &mut Universe, tree: TypeId, k: usize, part: TypeId) -> TypeId {
    if let Type::Or(l, r) = *u.types().get(tree)
        && !k.is_power_of_two()
    {
        let rest = k - (1 << k.ilog2());
        let r = appended(u, r, rest, part);
        return u.intern_type(Type::Or(l, r));
    }
    u.intern_type(Type::Or(tree, part))
}
