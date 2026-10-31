// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The type members of the types a type symbol stands for: of classes, intersections and
//! unions, with the order of their base classes.

use crate::host::Host;
use tasty::{SymbolId, Type, TypeId};

/// How many aliases `stood_for` follows at most.
const DEALIAS_STEPS: u32 = 100;

impl Host {
    /// `t` without the aliases, lambdas and upper bounds around the type it stands for.
    fn stood_for(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..DEALIAS_STEPS {
            t = match *self.u.types().get(t) {
                Type::Alias(a) => a,
                Type::Lambda { result, .. } => result,
                Type::Bounds(_, hi) => hi,
                _ => match self.u.dealias_step(t) {
                    next if next == t => return t,
                    next => next,
                },
            };
        }
        t
    }

    /// The type members of type `t`: of an intersection those of either side, of a union those
    /// of both (dotty's `&` and `|` of their denotations).
    pub fn members_of_type(&mut self, t: TypeId) -> Vec<SymbolId> {
        let t = self.stood_for(t);
        let named = |h: &Self, of: &[SymbolId], s: SymbolId| {
            of.iter().copied().find(|&o| h.u.symbol(o).name == h.u.symbol(s).name)
        };
        match *self.u.types().get(t) {
            Type::And(a, b) => {
                let (a, b) = (self.members_of_type(a), self.members_of_type(b));
                let mut found: Vec<SymbolId> = b.clone();
                for x in a {
                    match named(self, &b, x) {
                        Some(y) if self.owner_derives(x, y) => found.retain(|&f| f != y),
                        Some(_) => continue,
                        None => {}
                    }
                    found.push(x);
                }
                found
            }
            Type::Or(a, b, _) => {
                let (a, b) = (self.members_of_type(a), self.members_of_type(b));
                a.into_iter().filter_map(|x| named(self, &b, x).and_then(|y| self.joined(x, y))).collect()
            }
            _ => match self.u.class_symbol(t).filter(|&c| self.u.is_class(c)) {
                Some(c) => self.class_members(c, |k| k.is_type(), None),
                None => Vec::new(),
            },
        }
    }

    /// Whether the owner of `x` derives from the owner of `y`, another class.
    fn owner_derives(&mut self, x: SymbolId, y: SymbolId) -> bool {
        let (Some(a), Some(b)) = (self.u.symbol(x).owner, self.u.symbol(y).owner) else { return false };
        a != b && self.u.derives_from(a, b)
    }

    /// The member of a union both `x` and `y` stand for: the one of the base class, else one of
    /// that name of a class both owners derive from (dotty's `lubSym`).
    fn joined(&mut self, x: SymbolId, y: SymbolId) -> Option<SymbolId> {
        if x == y || self.owner_derives(y, x) {
            return Some(x);
        }
        if self.owner_derives(x, y) {
            return Some(y);
        }
        let (a, b) = (self.u.symbol(x).owner?, self.u.symbol(y).owner?);
        let name = self.u.symbol(x).name.clone();
        for c in self.u.linearization(a).into_iter().skip(1) {
            if !self.u.derives_from(b, c) {
                continue;
            }
            let decls = &self.u.symbol(c).decls;
            let found = decls
                .iter()
                .copied()
                .find(|&d| self.u.symbol(d).kind.is_type() && self.u.symbol(d).name == name);
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// The classes of type `t` and their parents in the order of dotty's `sortedParents`: of an
    /// intersection those of either side, of a union those of both.
    pub fn type_sorted_parents(&mut self, t: TypeId, order: &mut Vec<SymbolId>) {
        let t = self.stood_for(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => {
                self.type_sorted_parents(a, order);
                self.type_sorted_parents(b, order);
            }
            Type::Or(a, b, _) => {
                let (mut x, mut y) = (Vec::new(), Vec::new());
                self.type_sorted_parents(a, &mut x);
                self.type_sorted_parents(b, &mut y);
                for c in x {
                    if y.contains(&c) && !order.contains(&c) {
                        order.push(c);
                    }
                }
            }
            _ => {
                if let Some(c) = self.u.class_symbol(t).filter(|&c| self.u.is_class(c)) {
                    self.sorted_parents(c, order);
                }
            }
        }
    }
}
