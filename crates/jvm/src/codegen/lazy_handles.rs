// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
//! The `VarHandle`s of lazy values (dotty's `transformMemberDefThreadSafeNew`): their
//! initialization in `<clinit>` and the compare-and-set through them.

use super::body::Body;
use super::{Gen, unsupported};
use classfile::{FieldOp, Invoke};
use lower::Result;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

const VAR_HANDLE: &str = "Ljava/lang/invoke/VarHandle;";

impl Gen<'_, '_> {
    /// `handle = MethodHandles.lookup().findVarHandle(classOf[C], "x$lzyN", classOf[Object])` for
    /// each of `handles`, static fields of class `class`.
    pub fn init_var_handles(&mut self, b: &mut Body, class: &str, handles: &[SymbolId]) {
        let lookup = "java/lang/invoke/MethodHandles$Lookup";
        for &h in handles {
            let field = self.l.var_handle_field(h).expect("a lazy value's field");
            let lookup_desc = format!("()L{lookup};");
            b.code.invoke(Invoke::Static, b.pool, "java/lang/invoke/MethodHandles", "lookup", &lookup_desc);
            let this = b.pool.class(class);
            b.code.ldc(this);
            let name: Vec<u16> = tasty::encode(&self.l.u.symbol(field).name).encode_utf16().collect();
            let name = b.pool.string(&name);
            b.code.ldc(name);
            let object = b.pool.class("java/lang/Object");
            b.code.ldc(object);
            let find = format!("(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;){VAR_HANDLE}");
            b.code.invoke(Invoke::Virtual, b.pool, lookup, "findVarHandle", &find);
            let handle = tasty::encode(&self.l.u.symbol(h).name);
            b.code.field(FieldOp::PutStatic, b.pool, class, &handle, VAR_HANDLE);
        }
    }

    /// `objCAS(q.x$lzyN, expected, value)` as `x$lzyN$lzyHandle.compareAndSet(q, expected, value)`.
    pub fn var_handle_cas(&mut self, b: &mut Body, args: &[TNodeId]) -> Result<i32> {
        let &[field, expected, value] = args else { unreachable!("objCAS's three arguments") };
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(field) else {
            return unsupported(self.l.tpd.span(field), "a compare-and-set of no field");
        };
        let f = self.l.tpd.sym(field).expect("a field");
        let Some(handle) = self.l.var_handle(f) else {
            return unsupported(self.l.tpd.span(field), "a compare-and-set of a field without a VarHandle");
        };
        let owner = self.l.u.symbol(handle).owner.expect("a handle's class");
        let (owner, handle) = (self.class_name(owner), tasty::encode(&self.l.u.symbol(handle).name));
        b.code.field(FieldOp::GetStatic, b.pool, &owner, &handle, VAR_HANDLE);
        for t in [qualifier, expected, value] {
            self.expr(b, t)?;
        }
        let desc = "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z";
        b.code.invoke(Invoke::Virtual, b.pool, "java/lang/invoke/VarHandle", "compareAndSet", desc);
        Ok(1)
    }
}
