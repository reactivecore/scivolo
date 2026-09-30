//! Members of types (dotty's `Type.member`): through `this` and paths, of a class type, an
//! alias's or abstract type's bound, both sides of an intersection, the first base class common
//! to both sides of a union that declares the name. Refinements carry no symbols, so a
//! refinement's parent answers for it.

use super::{SymbolId, Universe};
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
            Type::Or(a, b) => self.union_member(a, b, name),
            Type::TermRef(..) | Type::Constant(..) | Type::Str(_) | Type::ByName(_) | Type::This(_) => {
                let widened = self.widen(t);
                match widened != t {
                    true => self.member_of_type_within(widened, name, steps),
                    false => Vec::new(),
                }
            }
            Type::Sym(_, s) if self.symbol(s).kind.is_term() => {
                let widened = self.widen(t);
                self.member_of_type_within(widened, name, steps)
            }
            Type::Refined(p, ..) | Type::Annotated(p) | Type::Rec(_, p) => {
                self.member_of_type_within(p, name, steps)
            }
            _ => match self.class_symbol(t) {
                Some(c) => self.member(c, name),
                None => Vec::new(),
            },
        }
    }

    /// The members named `name` of the first base class common to the classes of `a` and `b`
    /// that declares any.
    fn union_member(&mut self, a: TypeId, b: TypeId, name: &str) -> Vec<SymbolId> {
        let (wa, wb) = (self.widen(a), self.widen(b));
        let (Some(ca), Some(cb)) = (self.class_symbol(wa), self.class_symbol(wb)) else { return Vec::new() };
        for base in self.linearization(ca) {
            if !self.derives_from(cb, base) {
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
