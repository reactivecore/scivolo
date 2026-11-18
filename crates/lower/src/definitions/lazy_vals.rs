use super::{member, package};
use names::nme;
use tasty::{Kind, SymbolId, Universe};

/// What lazy values' accessors refer to: dotty's `scala.runtime.LazyVals` states.
#[derive(Clone, Copy)]
pub struct LazyVals {
    /// The objects `LazyVals.Evaluating` and `LazyVals.NullValue`.
    pub evaluating: SymbolId,
    pub null_value: SymbolId,
    /// The classes `LazyVals.LazyValControlState` and `LazyVals.Waiting`.
    pub control_state: SymbolId,
    pub waiting: SymbolId,
    pub waiting_ctor: SymbolId,
    /// `countDown` and `await` of `Waiting`, else of `CountDownLatch`, which it inherits them from.
    pub count_down: SymbolId,
    pub await_: SymbolId,
    /// `Boolean.unary_!`.
    pub not: SymbolId,
    /// `java.lang.invoke.VarHandle`.
    pub var_handle: SymbolId,
}

impl LazyVals {
    pub fn new(u: &mut Universe) -> Result<LazyVals, String> {
        let runtime = package(u, "scala.runtime")?;
        let lazy_vals = member(u, runtime, "LazyVals", Kind::Val)?;
        let lazy_vals = u.module_class(lazy_vals).ok_or("scala.runtime.LazyVals's class missing")?;
        let waiting = member(u, lazy_vals, "Waiting", Kind::Class)?;
        let latch = package(u, "java.util.concurrent")?;
        let latch = member(u, latch, "CountDownLatch", Kind::Class)?;
        let mut lock =
            |name| member(u, waiting, name, Kind::Def).or_else(|_| member(u, latch, name, Kind::Def));
        let (count_down, await_) = (lock("countDown")?, lock("await")?);
        let invoke = package(u, "java.lang.invoke")?;
        let boolean = member(u, u.defn().scala, "Boolean", Kind::Class)?;
        Ok(LazyVals {
            evaluating: member(u, lazy_vals, "Evaluating", Kind::Val)?,
            null_value: member(u, lazy_vals, "NullValue", Kind::Val)?,
            control_state: member(u, lazy_vals, "LazyValControlState", Kind::Class)?,
            waiting,
            waiting_ctor: member(u, waiting, nme::CONSTRUCTOR, Kind::Def)?,
            count_down,
            await_,
            not: member(u, boolean, nme::UNARY_NOT, Kind::Def)?,
            var_handle: member(u, invoke, "VarHandle", Kind::Class)?,
        })
    }
}

/// Scala Native's compare-and-set on a field.
#[derive(Clone, Copy)]
pub struct NativeCas {
    /// The object `scala.scalanative.runtime.LazyVals` and its `objCAS`.
    pub module: SymbolId,
    pub obj_cas: SymbolId,
}

impl NativeCas {
    pub fn new(u: &mut Universe) -> Result<NativeCas, String> {
        let runtime = package(u, "scala.scalanative.runtime")?;
        let module = member(u, runtime, "LazyVals", Kind::Val)?;
        let class = u.module_class(module).ok_or("scala.scalanative.runtime.LazyVals's class missing")?;
        Ok(NativeCas { module, obj_cas: member(u, class, nme::OBJ_CAS, Kind::Def)? })
    }
}

/// The JDK members lazy values use on the JVM (dotty's `MethodHandlesClass`,
/// `MethodHandles_lookup`, `MethodHandlesLookup_FindVarHandle` and `VarHandleClass`).
#[derive(Clone, Copy)]
pub struct VarHandles {
    /// The object of `MethodHandles`'s static members and its `lookup`.
    pub method_handles: SymbolId,
    pub lookup: SymbolId,
    /// `MethodHandles.Lookup.findVarHandle`.
    pub find_var_handle: SymbolId,
    /// `VarHandle.compareAndSet`, signature polymorphic.
    pub compare_and_set: SymbolId,
}

impl VarHandles {
    /// Finds the members in the JDK's class files; fails if the JDK lacks one or
    /// `compareAndSet` is not signature polymorphic there.
    pub fn new(u: &mut Universe) -> Result<VarHandles, String> {
        Self::find(u).map_err(|e| format!("lazy values on a JDK without VarHandle: {e}"))
    }

    fn find(u: &mut Universe) -> Result<VarHandles, String> {
        let invoke = package(u, "java.lang.invoke")?;
        let method_handles = member(u, invoke, "MethodHandles", Kind::Val)?;
        let statics = u.module_class(method_handles).ok_or("MethodHandles's class missing")?;
        let lookup_class = member(u, statics, "Lookup", Kind::Class)?;
        let var_handle = member(u, invoke, "VarHandle", Kind::Class)?;
        let compare_and_set = member(u, var_handle, "compareAndSet", Kind::Def)?;
        if !u.is_signature_polymorphic(compare_and_set) {
            return Err("VarHandle.compareAndSet is not signature polymorphic".into());
        }
        Ok(VarHandles {
            method_handles,
            lookup: member(u, statics, "lookup", Kind::Def)?,
            find_var_handle: member(u, lookup_class, "findVarHandle", Kind::Def)?,
            compare_and_set,
        })
    }
}
