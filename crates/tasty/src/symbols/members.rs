//! Members of types (dotty's `Type.member`): through `this` and paths, of a class type, through
//! aliases and upper bounds, both sides of an intersection, the first base class common to all
//! parts of a union that declares the name. Refinements carry no symbols, so a refinement's
//! parent answers for it.

use super::{Kind, SymbolId, Universe};
use crate::tags::PRIVATE;
use crate::types::{Type, TypeId};

/// How many types `member_of_type` looks through.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// The members named `name` of type `t`.
    pub fn member_of_type(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        self.member_of_type_within(t, name, MAX_STEPS)
    }

    fn member_of_type_within(&mut self, t: TypeId, name: &str, steps: u32) -> Vec<SymbolId> {
        let Some(steps) = steps.checked_sub(1) else { return Vec::new() };
        match self.types().get(t).clone() {
            Type::And(a, b) => {
                let mut found = self.member_of_type_within(a, name, steps);
                for m in self.member_of_type_within(b, name, steps) {
                    if !found.contains(&m) {
                        found.push(m);
                    }
                }
                found
            }
            Type::Or(..) => {
                let mut classes = Vec::new();
                match self.union_classes(t, &mut classes, steps) {
                    true => self.common_member(&classes, name),
                    false => Vec::new(),
                }
            }
            Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Rec(_, p) => {
                self.member_of_type_within(p, name, steps)
            }
            _ => {
                if let Some(c) = self.class_symbol(t) {
                    return self.member(c, name);
                }
                match self.bound_of(t) {
                    Some(u) => self.member_of_type_within(u, name, steps),
                    None => Vec::new(),
                }
            }
        }
    }

    /// The type `t` stands for when it has no class of its own: a path's or constant's
    /// widened type, an alias's right-hand side, an abstract type's upper bound.
    fn bound_of(&mut self, t: TypeId) -> Option<TypeId> {
        let widened = self.widen(t);
        if widened != t {
            return Some(widened);
        }
        let dealiased = self.dealias(t);
        if dealiased != t {
            return Some(dealiased);
        }
        let s = match self.types().get(t).clone() {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.resolve_ref(t)?,
            Type::Bounds(_, hi) => return Some(hi),
            _ => return None,
        };
        if !matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) {
            return None;
        }
        let info = self.info(s);
        match self.types().get(info).clone() {
            Type::Bounds(_, hi) => Some(hi),
            _ => None,
        }
    }

    /// Adds the class of each part of union `t` to `classes`; `false` if a part has none.
    fn union_classes(&mut self, t: TypeId, classes: &mut Vec<SymbolId>, steps: u32) -> bool {
        let Some(steps) = steps.checked_sub(1) else { return false };
        match self.types().get(t).clone() {
            Type::Or(a, b, _) => {
                self.union_classes(a, classes, steps) && self.union_classes(b, classes, steps)
            }
            Type::And(a, _) | Type::Refined(a, ..) | Type::Annotated(a, _) | Type::Rec(_, a) => {
                self.union_classes(a, classes, steps)
            }
            _ => match self.class_symbol(t) {
                Some(c) => {
                    classes.push(c);
                    true
                }
                None => self.bound_of(t).is_some_and(|u| self.union_classes(u, classes, steps)),
            },
        }
    }

    /// The members named `name` of the first base class common to `classes` that declares any.
    fn common_member(&mut self, classes: &[SymbolId], name: &str) -> Vec<SymbolId> {
        let Some(&first) = classes.first() else { return Vec::new() };
        for base in self.linearization(first) {
            if !classes.iter().all(|&c| self.derives_from(c, base)) {
                continue;
            }
            let found = self.member_where(base, name, |u, m| !u.symbol(m).flags.has(PRIVATE));
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }
}
