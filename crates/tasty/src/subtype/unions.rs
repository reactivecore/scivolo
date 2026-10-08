//! Unions balanced as dotty's `OrType.balanced` builds them, memoised on shared lists of their
//! parts, and the atoms of unions (dotty's `myAtoms`) on such lists.

use super::atoms::{Atoms, Range};
use crate::Universe;
use crate::types::{Type, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

/// A list of types: the first `.1` of shared list `.0`, or a list of its own.
#[derive(Clone, PartialEq, Debug)]
pub enum List {
    Shared(u32, u32),
    Own(Vec<TypeId>),
}

#[derive(Default)]
pub struct Unions {
    /// Lists that only grow at their end, each shared by the lists that are prefixes of it.
    lists: Vec<Vec<TypeId>>,
    /// Per list, the first position of each type in it.
    positions: Vec<FxHashMap<TypeId, u32>>,
    /// The parts of unions, as `balanced` flattens them.
    parts: FxHashMap<TypeId, List>,
    /// The balanced union of the parts of a list from a start, of a length.
    built: FxHashMap<(u32, u32, u32), TypeId>,
    /// The atoms of unions that depend on neither a constraint nor a symbol.
    atoms: FxHashMap<TypeId, Atoms>,
}

impl Universe {
    /// `a | b`, rebuilt balanced if one side has more than one part more than the other.
    pub fn union(&mut self, a: TypeId, b: TypeId) -> TypeId {
        let mut unions = std::mem::take(self.unions());
        let t = unions.union(self, a, b);
        *self.unions() = unions;
        t
    }
}

impl Unions {
    pub fn items<'a>(&'a self, l: &'a List) -> &'a [TypeId] {
        match l {
            List::Shared(i, n) => &self.lists[*i as usize][..*n as usize],
            List::Own(v) => v,
        }
    }

    /// Whether every type of `xs` is one of `ys`.
    pub fn subset(&self, xs: &List, ys: &List) -> bool {
        let xs = self.items(xs);
        let test = self.members(ys, xs.len());
        xs.iter().all(|&x| test(x))
    }

    /// Whether `xs` and `ys` have no type in common.
    pub fn disjoint(&self, xs: &List, ys: &List) -> bool {
        let ys_items = self.items(ys);
        let test = self.members(xs, ys_items.len());
        !ys_items.iter().any(|&y| test(y))
    }

    /// The type `xs` consists of, if it is not empty and has no other.
    pub fn one(&self, xs: &List) -> Option<TypeId> {
        let (&x, rest) = self.items(xs).split_first()?;
        rest.iter().all(|&y| y == x).then_some(x)
    }

    /// The types of `xs` that are in `ys`.
    pub fn common(&self, xs: &List, ys: &List) -> List {
        let xs = self.items(xs);
        let test = self.members(ys, xs.len());
        List::Own(xs.iter().copied().filter(|&x| test(x)).collect())
    }

    /// `xs` followed by `ys`, shared if `share`.
    pub fn joined(&mut self, xs: &List, ys: &List, share: bool) -> List {
        match share {
            true => self.concat(xs, ys),
            false => List::Own([self.items(xs), self.items(ys)].concat()),
        }
    }

    /// A test for membership in `ys`, which `queries` tests will use.
    fn members<'a>(&'a self, ys: &'a List, queries: usize) -> impl Fn(TypeId) -> bool + 'a {
        let set: Option<FxHashSet<TypeId>> = match ys {
            List::Own(v) if v.len() > 16 && queries > 16 => Some(v.iter().copied().collect()),
            _ => None,
        };
        move |x| match (ys, &set) {
            (List::Shared(i, n), _) => self.positions[*i as usize].get(&x).is_some_and(|p| p < n),
            (_, Some(s)) => s.contains(&x),
            (List::Own(v), None) => v.contains(&x),
        }
    }

    /// `xs` followed by `ys` as a shared list: `xs` extended in place if it ends its list.
    pub fn concat(&mut self, xs: &List, ys: &List) -> List {
        let tail = self.items(ys).to_vec();
        let (i, n) = match *xs {
            List::Shared(i, n) if self.lists[i as usize].len() == n as usize => (i, n),
            _ => {
                let head = self.items(xs).to_vec();
                self.lists.push(Vec::new());
                self.positions.push(FxHashMap::default());
                let i = self.lists.len() as u32 - 1;
                self.push(i, &head);
                (i, head.len() as u32)
            }
        };
        self.push(i, &tail);
        List::Shared(i, n + tail.len() as u32)
    }

    fn shared(&mut self, l: &List) -> List {
        match l {
            List::Shared(..) => l.clone(),
            List::Own(_) => self.concat(l, &List::Own(Vec::new())),
        }
    }

    fn push(&mut self, i: u32, items: &[TypeId]) {
        let (list, positions) = (&mut self.lists[i as usize], &mut self.positions[i as usize]);
        for &t in items {
            positions.entry(t).or_insert(list.len() as u32);
            list.push(t);
        }
    }

    pub fn atoms(&self, t: TypeId) -> Option<Atoms> {
        self.atoms.get(&t).cloned()
    }

    /// Records `atoms` as the atoms of union `t`, their lists shared.
    pub fn set_atoms(&mut self, t: TypeId, atoms: Atoms) {
        if self.atoms.contains_key(&t) {
            return;
        }
        let atoms = match atoms {
            Atoms::Known(r) => {
                let lo = self.shared(&r.lo);
                let hi = if r.hi == r.lo { lo.clone() } else { self.shared(&r.hi) };
                Atoms::Known(Range { lo, hi })
            }
            other => other,
        };
        self.atoms.insert(t, atoms);
    }

    fn union(&mut self, u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
        let (pa, pb) = (self.parts_of(u, a), self.parts_of(u, b));
        let (na, nb) = (self.items(&pa).len(), self.items(&pb).len());
        let all = self.concat(&pa, &pb);
        let t = match all {
            _ if na.abs_diff(nb) <= 1 => u.intern_type(Type::Or(a, b)),
            List::Shared(i, n) => self.built(u, i, 0, n),
            List::Own(_) => unreachable!("concat shares its result"),
        };
        self.parts.entry(t).or_insert(all);
        t
    }

    fn parts_of(&mut self, u: &Universe, t: TypeId) -> List {
        if !matches!(u.types().get(t), Type::Or(..)) {
            return List::Own(vec![t]);
        }
        if let Some(p) = self.parts.get(&t) {
            return p.clone();
        }
        let mut flat = Vec::new();
        or_parts(u, t, &mut flat);
        let p = self.shared(&List::Own(flat));
        self.parts.insert(t, p.clone());
        p
    }

    /// The balanced union of the `len` parts of list `i` from `start`.
    fn built(&mut self, u: &mut Universe, i: u32, start: u32, len: u32) -> TypeId {
        if len == 1 {
            return self.lists[i as usize][start as usize];
        }
        if let Some(&t) = self.built.get(&(i, start, len)) {
            return t;
        }
        let left = len / 2;
        let (x, y) = (self.built(u, i, start, left), self.built(u, i, start + left, len - left));
        let t = u.intern_type(Type::Or(x, y));
        self.built.insert((i, start, len), t);
        t
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
