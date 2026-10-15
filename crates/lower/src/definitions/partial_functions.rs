//! What closures of `PartialFunction` types expand to: the methods an anonymous class overrides,
//! its parents and the `apply` its `applyOrElse` calls on the default function.

use super::{member, package};
use tasty::{Kind, SymbolId, Universe};

#[derive(Clone, Copy)]
pub struct PartialFunctions {
    pub class: SymbolId,
    pub is_defined_at: SymbolId,
    pub apply_or_else: SymbolId,
    /// `scala.runtime.AbstractPartialFunction` and its constructor.
    pub abstract_class: SymbolId,
    pub abstract_ctor: SymbolId,
    pub serializable: SymbolId,
    pub function1_apply: SymbolId,
}

impl PartialFunctions {
    pub fn new(u: &mut Universe) -> Result<PartialFunctions, String> {
        let scala = package(u, "scala")?;
        let class = member(u, scala, "PartialFunction", Kind::Class)?;
        let abstract_class = member(u, package(u, "scala.runtime")?, "AbstractPartialFunction", Kind::Class)?;
        let function1 = member(u, scala, "Function1", Kind::Class)?;
        Ok(PartialFunctions {
            class,
            is_defined_at: member(u, class, "isDefinedAt", Kind::Def)?,
            apply_or_else: member(u, class, "applyOrElse", Kind::Def)?,
            abstract_class,
            abstract_ctor: member(u, abstract_class, "<init>", Kind::Def)?,
            serializable: member(u, package(u, "java.io")?, "Serializable", Kind::Class)?,
            function1_apply: member(u, function1, "apply", Kind::Def)?,
        })
    }
}
