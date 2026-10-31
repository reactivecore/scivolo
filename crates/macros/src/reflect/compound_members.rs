// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The type members of the types a type symbol stands for: of classes, intersections and
//! unions, with the order of their base classes.

use crate::host::Host;
use tasty::tags::TRAIT;
use tasty::{Kind, SymbolId, Type, TypeId};

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

    /// The type members of type `t`: those of the classes it stands for, of two members of a
    /// name the one dotty's `Denotation.meet` keeps.
    pub fn members_of_type(&mut self, t: TypeId) -> Vec<SymbolId> {
        let pre = self.type_base_classes(t);
        let mut found: Vec<SymbolId> = Vec::new();
        for c in self.classes_of(t) {
            for x in self.class_members(c, |k| k.is_type(), None) {
                let name = &self.u.symbol(x).name;
                match found.iter().position(|&f| self.u.symbol(f).name == *name) {
                    Some(k) if self.sym_score(found[k], x, &pre) < 0 => found[k] = x,
                    Some(_) => {}
                    None => found.push(x),
                }
            }
        }
        found
    }

    /// The classes whose intersection `t` stands for, a union by its join (dotty's
    /// `orDominator`).
    fn classes_of(&mut self, t: TypeId) -> Vec<SymbolId> {
        let t = self.stood_for(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => {
                let mut out = self.classes_of(a);
                out.extend(self.classes_of(b));
                out
            }
            Type::Or(..) => {
                let mut dominators: Vec<SymbolId> = Vec::new();
                for c in self.or_base_classes(t) {
                    if !dominators.iter().any(|&d| self.u.derives_from(d, c)) {
                        dominators.insert(0, c);
                    }
                }
                dominators
            }
            _ => self.u.class_symbol(t).filter(|&c| self.u.is_class(c)).into_iter().collect(),
        }
    }

    /// The base classes both sides of union `t` share, in the order of its left side's.
    fn or_base_classes(&mut self, t: TypeId) -> Vec<SymbolId> {
        let t = self.stood_for(t);
        let Type::Or(a, b, _) = *self.u.types().get(t) else { return self.type_base_classes(t) };
        let right = self.or_base_classes(b);
        self.or_base_classes(a).into_iter().filter(|c| right.contains(c)).collect()
    }

    /// dotty's `baseClasses` of `t`: of an intersection its right side's not in its left side's
    /// first, of a union its right side's in its left side's.
    fn type_base_classes(&mut self, t: TypeId) -> Vec<SymbolId> {
        let t = self.stood_for(t);
        let (and, l, r) = match *self.u.types().get(t) {
            Type::And(l, r) => (true, l, r),
            Type::Or(l, r, _) => (false, l, r),
            _ => {
                let c = self.u.class_symbol(t).filter(|&c| self.u.is_class(c));
                return c.map(|c| self.u.linearization(c)).unwrap_or_default();
            }
        };
        let (left, right) = (self.type_base_classes(l), self.type_base_classes(r));
        let mut out = Vec::new();
        for (i, &c) in right.iter().enumerate() {
            match (left.contains(&c), self.u.symbol(c).flags.has(TRAIT)) {
                (false, _) if and => out.push(c),
                (false, _) => {}
                (true, true) if !and => out.push(c),
                (true, true) => {}
                (true, false) if and => return [out, left].concat(),
                (true, false) => return [out, right[i..].to_vec()].concat(),
            }
        }
        if and { [out, left].concat() } else { out }
    }

    /// Positive to keep type member `a`, negative to keep `b` (dotty's `symScore`): the concrete
    /// one, else the one whose owner derives from the other's or comes first in `pre`.
    fn sym_score(&mut self, a: SymbolId, b: SymbolId, pre: &[SymbolId]) -> i32 {
        let (da, db) = (self.is_abstract_type(a), self.is_abstract_type(b));
        if da != db {
            return if da { -1 } else { 1 };
        }
        let (Some(oa), Some(ob)) = (self.u.symbol(a).owner, self.u.symbol(b).owner) else { return 0 };
        if oa == ob {
            0
        } else if self.u.derives_from(oa, ob) {
            1
        } else if self.u.derives_from(ob, oa) {
            -1
        } else {
            match pre.iter().find(|&&c| c == oa || c == ob) {
                Some(&c) if c == oa => 1,
                Some(_) => -1,
                None => 0,
            }
        }
    }

    /// Whether type member `s` is a type parameter or abstract (dotty's `Deferred`).
    fn is_abstract_type(&mut self, s: SymbolId) -> bool {
        self.u.symbol(s).kind == Kind::TypeParam || self.is_deferred(s)
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
