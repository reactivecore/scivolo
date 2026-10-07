use super::{member, package};
use tasty::{Kind, SymbolId, Universe};

/// What lazy values' accessors refer to: dotty's `scala.runtime.LazyVals` states and Scala
/// Native's compare-and-set on a field.
#[derive(Clone, Copy)]
pub struct LazyVals {
    /// The objects `LazyVals.Evaluating` and `LazyVals.NullValue`.
    pub evaluating: SymbolId,
    pub null_value: SymbolId,
    /// The classes `LazyVals.LazyValControlState` and `LazyVals.Waiting`.
    pub control_state: SymbolId,
    pub waiting: SymbolId,
    pub waiting_ctor: SymbolId,
    /// `CountDownLatch.countDown` and `await` that `Waiting` inherits.
    pub count_down: SymbolId,
    pub await_: SymbolId,
    /// The object `scala.scalanative.runtime.LazyVals` and its `objCAS`.
    pub native: SymbolId,
    pub obj_cas: SymbolId,
    /// `Object.eq` and `Boolean.unary_!`.
    pub eq: SymbolId,
    pub not: SymbolId,
    /// `java.lang.invoke.VarHandle`.
    pub var_handle: SymbolId,
}

impl LazyVals {
    pub fn new(u: &mut Universe, object: SymbolId) -> Result<LazyVals, String> {
        let runtime = package(u, "scala.runtime")?;
        let lazy_vals = member(u, runtime, "LazyVals", Kind::Val)?;
        let lazy_vals = u.module_class(lazy_vals).ok_or("scala.runtime.LazyVals's class missing")?;
        let waiting = member(u, lazy_vals, "Waiting", Kind::Class)?;
        let latch = package(u, "java.util.concurrent")?;
        let latch = member(u, latch, "CountDownLatch", Kind::Class)?;
        let native_runtime = package(u, "scala.scalanative.runtime")?;
        let native = member(u, native_runtime, "LazyVals", Kind::Val)?;
        let native_class =
            u.module_class(native).ok_or("scala.scalanative.runtime.LazyVals's class missing")?;
        let invoke = package(u, "java.lang.invoke")?;
        let scala = package(u, "scala")?;
        let boolean = member(u, scala, "Boolean", Kind::Class)?;
        Ok(LazyVals {
            evaluating: member(u, lazy_vals, "Evaluating", Kind::Val)?,
            null_value: member(u, lazy_vals, "NullValue", Kind::Val)?,
            control_state: member(u, lazy_vals, "LazyValControlState", Kind::Class)?,
            waiting,
            waiting_ctor: member(u, waiting, "<init>", Kind::Def)?,
            count_down: member(u, latch, "countDown", Kind::Def)?,
            await_: member(u, latch, "await", Kind::Def)?,
            native,
            obj_cas: member(u, native_class, "objCAS", Kind::Def)?,
            eq: member(u, object, "eq", Kind::Def)?,
            not: member(u, boolean, "unary_!", Kind::Def)?,
            var_handle: member(u, invoke, "VarHandle", Kind::Class)?,
        })
    }
}
