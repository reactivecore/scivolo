// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Ops.scala
//   nir/src/main/scala/scala/scalanative/nir/Vals.scala
//! Operations bound to fresh locals, and the types of operations and values (NIR's `resty`
//! and `ty`).

use super::Gen;
use crate::definitions::boxed_class;
use nir::{Inst, Op, Type, TypeId, Val, ValId};

impl Gen<'_, '_> {
    /// `let %n = op`, with the current unwind edge; the value `%n`.
    pub fn emit(&mut self, op: Op) -> ValId {
        let unwind = self.unwind();
        let id = self.fresh();
        self.bind(id, op, unwind)
    }

    /// `let %n = op` with `%n` named `name`, drawn before the unwind edge.
    pub fn emit_named(&mut self, op: Op, name: &str) -> ValId {
        let id = self.fresh_named(name);
        let unwind = self.unwind();
        self.bind(id, op, unwind)
    }

    fn bind(&mut self, id: nir::Local, op: Op, unwind: nir::Next) -> ValId {
        let ty = self.resty(&op);
        let scope = self.m.scope;
        self.emit_inst(Inst::Let { id, op, unwind, scope });
        self.val(Val::Local(id, ty))
    }

    pub fn resty(&mut self, op: &Op) -> TypeId {
        match *op {
            Op::Call { ty, .. } => match self.pool.ty(ty) {
                Type::Function(_, ret) => *ret,
                t => unreachable!("a call of type {t:?}"),
            },
            Op::Classalloc { name, .. } | Op::Module(name) => {
                self.ty(Type::Ref { name, exact: true, nullable: false })
            }
            Op::Method { .. } | Op::Dynmethod { .. } | Op::Field { .. } | Op::Elem { .. } => {
                self.ty(Type::Ptr)
            }
            Op::As { ty, .. } | Op::Fieldload { ty, .. } | Op::Load { ty, .. } | Op::Arrayload { ty, .. } => {
                ty
            }
            Op::Box { ty, .. } => match *self.pool.ty(ty) {
                Type::Ref { name, .. } => self.ty(Type::Ref { name, exact: true, nullable: false }),
                ref t => unreachable!("a box of type {t:?}"),
            },
            Op::Bin { ty, .. } | Op::Conv { ty, .. } => ty,
            Op::Is { .. } | Op::Comp { .. } => self.ty(Type::Bool),
            Op::Copy(v) => self.val_ty(v),
            Op::Var(ty) => self.ty(Type::Var(ty)),
            Op::Varload(slot) => {
                let slot_ty = self.val_ty(slot);
                match *self.pool.ty(slot_ty) {
                    Type::Var(ty) => ty,
                    ref t => unreachable!("a variable of type {t:?}"),
                }
            }
            Op::Arrayalloc { ty, .. } => self.ty(Type::Array { elem: ty, nullable: false }),
            Op::Arraylength(_) => self.ty(Type::Int),
            Op::SizeOf(_) | Op::AlignmentOf(_) => self.ty(Type::Size),
            Op::Store { .. } | Op::Fieldstore { .. } | Op::Varstore { .. } | Op::Arraystore { .. } => {
                self.ty(Type::Unit)
            }
            Op::Fence(_) => self.ty(Type::Unit),
            Op::Stackalloc { .. } => self.ty(Type::Ptr),
            Op::Unbox { ty, .. } => {
                let Type::Ref { name, .. } = *self.pool.ty(ty) else { unreachable!("a box class") };
                let class = self.pool.top_name(name);
                let letter = "zcbsijfd".chars().find(|&c| boxed_class(c) == Some(class));
                self.erased(&tasty::Erased::Prim(letter.expect("the class of a boxed primitive")))
            }
            Op::Extract { .. } | Op::Insert { .. } => {
                unreachable!("not generated yet: {op:?}")
            }
        }
    }

    pub fn resty_is_nothing(&mut self, op: &Op) -> bool {
        let ty = self.resty(op);
        *self.pool.ty(ty) == Type::Nothing
    }

    pub fn val_ty(&mut self, v: ValId) -> TypeId {
        let t = match self.pool.val(v).clone() {
            Val::True | Val::False => Type::Bool,
            Val::Null => Type::Null,
            Val::Unit => Type::Unit,
            Val::Zero(ty) | Val::Local(_, ty) | Val::Global(_, ty) => return ty,
            Val::Char(_) => Type::Char,
            Val::Byte(_) => Type::Byte,
            Val::Short(_) => Type::Short,
            Val::Int(_) => Type::Int,
            Val::Long(_) => Type::Long,
            Val::Float(_) => Type::Float,
            Val::Double(_) => Type::Double,
            Val::Size(_) => Type::Size,
            Val::String(_) => return self.class_type("java.lang.String", true),
            Val::ClassOf(_) => return self.class_type("java.lang.Class", false),
            v => unreachable!("not generated yet: {v:?}"),
        };
        self.ty(t)
    }

    /// The reference type of class `name`, exact and non-null if `exact`.
    pub fn class_type(&mut self, name: &str, exact: bool) -> TypeId {
        let name = self.pool.top(name);
        self.ty(Type::Ref { name, exact, nullable: !exact })
    }

    pub fn throwable_type(&mut self) -> TypeId {
        self.class_type("java.lang.Throwable", false)
    }
}
