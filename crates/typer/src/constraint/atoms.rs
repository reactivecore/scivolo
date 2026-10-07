//! Unions of constants as lower bounds (dotty's `Atoms`).

use tasty::{Type, TypeId, Universe};

/// The constants of `t` if it is a constant or a union of constants (dotty's `Atoms`).
fn atoms(u: &Universe, t: TypeId, out: &mut Vec<TypeId>) -> bool {
    match *u.types().get(t) {
        Type::Or(a, b) => atoms(u, a, out) && atoms(u, b, out),
        Type::Constant(..) | Type::Str(_) => {
            out.push(t);
            true
        }
        _ => false,
    }
}

/// The union of `parts`, balanced (dotty's `OrType.balanced`).
fn balanced_union(u: &mut Universe, parts: &[TypeId]) -> TypeId {
    if let [t] = parts {
        return *t;
    }
    let (a, b) = parts.split_at(parts.len() / 2);
    let (a, b) = (balanced_union(u, a), balanced_union(u, b));
    u.intern_type(Type::Or(a, b))
}

/// The least upper bound of `a` and `b` if both are constants or unions of constants.
pub fn atoms_lub(u: &mut Universe, a: TypeId, b: TypeId) -> Option<TypeId> {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    if !atoms(u, a, &mut xs) || !atoms(u, b, &mut ys) {
        return None;
    }
    if ys.iter().all(|y| xs.contains(y)) {
        return Some(a);
    }
    if xs.iter().all(|x| ys.contains(x)) {
        return Some(b);
    }
    if ys.iter().any(|y| xs.contains(y)) {
        return None;
    }
    let left = xs.len();
    xs.extend(ys);
    Some(if left.abs_diff(xs.len() - left) <= 1 {
        u.intern_type(Type::Or(a, b))
    } else {
        balanced_union(u, &xs)
    })
}
