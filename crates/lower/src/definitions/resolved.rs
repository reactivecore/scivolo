//! The definitions resolved on first use, each kept once resolved.

use super::{
    CaseClasses, Equality, LazyVals, Monitors, PartialFunctions, Patmat, Sequences, Strings, member, package,
};
use crate::{Lower, Result, Unsupported};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Universe};

/// What an object's `writeReplace` refers to: `java.lang.Class`,
/// `scala.runtime.ModuleSerializationProxy` and its constructor.
#[derive(Clone, Copy)]
pub struct Serialization {
    pub class: SymbolId,
    pub proxy: SymbolId,
    pub proxy_ctor: SymbolId,
}

impl Lower<'_> {
    pub fn equality(&mut self) -> Result<Equality> {
        let (any, object) = (self.defs.any, self.defs.object);
        resolved(&mut self.defs.equality, self.u, |u| Equality::new(u, any, object))
    }

    pub fn strings(&mut self) -> Result<Strings> {
        resolved(&mut self.defs.strings, self.u, Strings::new)
    }

    pub fn patmat(&mut self) -> Result<Patmat> {
        resolved(&mut self.defs.patmat, self.u, Patmat::new)
    }

    pub fn case_classes(&mut self) -> Result<CaseClasses> {
        let equality = self.equality()?;
        let (any, object) = (self.defs.any, self.defs.object);
        resolved(&mut self.defs.case_classes, self.u, |u| CaseClasses::new(u, any, object, equality))
    }

    pub fn lazy_vals(&mut self) -> Result<LazyVals> {
        resolved(&mut self.defs.lazy_vals, self.u, LazyVals::new)
    }

    pub fn monitors(&mut self) -> Result<Monitors> {
        resolved(&mut self.defs.monitors, self.u, Monitors::new)
    }

    pub fn sequences(&mut self) -> Result<Sequences> {
        resolved(&mut self.defs.sequences, self.u, Sequences::new)
    }

    pub fn serialization(&mut self) -> Result<Serialization> {
        resolved(&mut self.defs.serialization, self.u, |u| {
            let class = member(u, package(u, "java.lang")?, "Class", Kind::Class)?;
            let proxy = member(u, package(u, "scala.runtime")?, "ModuleSerializationProxy", Kind::Class)?;
            Ok(Serialization { class, proxy, proxy_ctor: member(u, proxy, "<init>", Kind::Def)? })
        })
    }

    pub fn partial_functions(&mut self) -> Result<PartialFunctions> {
        resolved(&mut self.defs.partial_functions, self.u, PartialFunctions::new)
    }

    /// The trait `scala.Product`.
    pub fn product(&mut self) -> Result<SymbolId> {
        resolved(&mut self.defs.product, self.u, |u| member(u, package(u, "scala")?, "Product", Kind::Class))
    }

    /// The object `scala.runtime.ScalaRunTime`.
    pub fn scala_run_time(&mut self) -> Result<SymbolId> {
        resolved(&mut self.defs.scala_run_time, self.u, |u| {
            member(u, package(u, "scala.runtime")?, "ScalaRunTime", Kind::Val)
        })
    }

    /// `Array.clone`, which erasure calls `Object.clone` instead of.
    pub fn array_clone(&mut self) -> Result<SymbolId> {
        resolved(&mut self.defs.array_clone, self.u, |u| {
            let array = member(u, package(u, "scala")?, "Array", Kind::Class)?;
            member(u, array, "clone", Kind::Def)
        })
    }
}

/// `slot`, else what `new` resolves, kept in `slot`.
fn resolved<T: Clone>(
    slot: &mut Option<T>,
    u: &mut Universe,
    new: impl FnOnce(&mut Universe) -> std::result::Result<T, String>,
) -> Result<T> {
    if let Some(t) = slot {
        return Ok(t.clone());
    }
    let t = new(u).map_err(|what| Unsupported { at: Span::NONE, what })?;
    *slot = Some(t.clone());
    Ok(t)
}
