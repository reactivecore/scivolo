use super::{Changes, Mark, TypeVars};
use crate::types::{TypeId, VarId};
use crate::{ANY, NOTHING, Universe};

/// No constraint: a type variable is a subtype and a supertype of itself only.
pub struct NoVars;

impl TypeVars for NoVars {
    fn instance(&self, _: VarId) -> Option<TypeId> {
        None
    }
    fn add_lower(&mut self, _: &mut Universe, _: VarId, _: TypeId) -> bool {
        false
    }
    fn add_upper(&mut self, _: &mut Universe, _: VarId, _: TypeId) -> bool {
        false
    }
    fn bounds(&self, _: VarId) -> (TypeId, TypeId) {
        (NOTHING, ANY)
    }
    fn frozen(&self) -> bool {
        true
    }
    fn mark(&self) -> Mark {
        Mark(0)
    }
    fn rollback(&mut self, _: Mark) {}
    fn changes_since(&self, _: Mark) -> Changes {
        Changes { vars: Vec::new(), redo: Box::new(()) }
    }
    fn redo(&mut self, _: Changes) {}
}
