use super::member;
use tasty::{Kind, SymbolId, Universe};

/// What `ArrayApply` builds lists from: `Nil` and `::`.
#[derive(Clone, Copy)]
pub struct Sequences {
    pub nil: SymbolId,
    pub cons: SymbolId,
    pub cons_ctor: SymbolId,
}

impl Sequences {
    pub fn new(u: &mut Universe) -> Result<Sequences, String> {
        let immutable = super::package(u, "scala.collection.immutable")?;
        let cons = member(u, immutable, "::", Kind::Class)?;
        Ok(Sequences {
            nil: member(u, immutable, "Nil", Kind::Val)?,
            cons,
            cons_ctor: member(u, cons, "<init>", Kind::Def)?,
        })
    }
}
