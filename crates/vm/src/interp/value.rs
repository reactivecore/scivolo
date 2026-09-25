use super::{Flow, Func, FuncId, func_ref};
use crate::heap;
use crate::layout::{self, WORD};
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, Type, TypeId, Val, ValId};

impl Vm {
    /// The value of operand `v`, as the zero-extended bits of its type.
    pub fn val(&mut self, v: ValId, slots: &[u64]) -> Flow {
        let pool = &self.world.pool;
        Ok(match pool.val(v) {
            Val::Local(l, _) => slots[l.0 as usize],
            Val::True => 1,
            Val::False | Val::Null | Val::Zero(_) => 0,
            Val::Int(i) => *i as u32 as u64,
            Val::Long(l) => *l as u64,
            Val::Char(c) => *c as u64,
            Val::Byte(b) => *b as u8 as u64,
            Val::Short(s) => *s as u16 as u64,
            Val::Float(f) => *f as u64,
            Val::Double(d) | Val::Size(d) => *d,
            Val::String(s) => {
                let s = *s;
                self.string_literal(s)? as u64
            }
            Val::Global(g, _) => {
                let g = *g;
                self.global_value(g)?
            }
            Val::ClassOf(g) => {
                let g = *g;
                let cls = self.link(g)?;
                self.class(cls).rtti as u64
            }
            Val::Unit => self.unit()? as u64,
            Val::Const(_) => self.constant(v)? as u64,
            other => return Err(VmError::Unsupported(format!("value {other:?}")).into()),
        })
    }

    /// The type of operand `v`.
    pub fn val_ty(&mut self, v: ValId) -> Type {
        match self.world.pool.val(v) {
            Val::Local(_, ty) | Val::Global(_, ty) | Val::Zero(ty) => self.world.pool.ty(*ty).clone(),
            Val::True | Val::False => Type::Bool,
            Val::Int(_) => Type::Int,
            Val::Long(_) => Type::Long,
            Val::Char(_) => Type::Char,
            Val::Byte(_) => Type::Byte,
            Val::Short(_) => Type::Short,
            Val::Float(_) => Type::Float,
            Val::Double(_) => Type::Double,
            Val::Size(_) => Type::Size,
            _ => Type::Ptr,
        }
    }

    pub fn unit(&mut self) -> Flow<usize> {
        let g = self.world.top("scala.scalanative.runtime.BoxedUnit$");
        self.module(g)
    }

    /// A function reference for methods and externs, the storage address for variables.
    fn global_value(&mut self, g: GlobalId) -> Flow {
        if let Some(&addr) = self.funcs.globals.get(&g) {
            return Ok(addr as u64);
        }
        if let Some(&f) = self.funcs.by_name.get(&g) {
            return self.callable(f);
        }
        if self.native(g).is_none() {
            let defn = self.world.defn(g)?;
            let is_extern = defn.attrs.iter().any(nir::Attr::is_extern);
            let addr = match defn.kind {
                DefnKind::Var { .. } | DefnKind::Const { .. } if is_extern => self.extern_variable(g)?,
                DefnKind::Var { ty, rhs } | DefnKind::Const { ty, rhs } => {
                    let addr = self.heap.alloc(layout::size_of(&self.world.pool, ty) as usize);
                    let value = self.val(rhs, &[])?;
                    self.store_ty(addr, ty, value);
                    addr
                }
                DefnKind::Declare { ty } if !matches!(self.world.pool.ty(ty), Type::Function(..)) => {
                    self.extern_variable(g)?
                }
                _ => {
                    let f = self.func(g)?;
                    return self.callable(f);
                }
            };
            self.funcs.globals.insert(g, addr);
            return Ok(addr as u64);
        }
        Ok(func_ref(self.func(g)?))
    }

    /// The value code holds for function `f`: a C trampoline for extern definitions, which C
    /// may call, and a function reference otherwise.
    pub fn callable(&mut self, f: FuncId) -> Flow {
        let Func::Nir { defn, name, .. } = &self.funcs.all[f as usize] else { return Ok(func_ref(f)) };
        if !defn.attrs.iter().any(nir::Attr::is_extern) {
            return Ok(func_ref(f));
        }
        let name = *name;
        let arity = match &defn.kind {
            DefnKind::Define { ty, .. } => match self.world.pool.ty(*ty) {
                Type::Function(args, _) => args.len(),
                _ => 0,
            },
            _ => 0,
        };
        match self.callbacks.address(f, arity) {
            Some(addr) => Ok(addr as u64),
            None => Err(VmError::Unsupported(format!("C callback {}", self.world.name(name))).into()),
        }
    }

    /// The address of constant data `v`, laid out in memory once.
    fn constant(&mut self, v: ValId) -> Flow<usize> {
        if let Some(&addr) = self.funcs.consts.get(&v) {
            return Ok(addr);
        }
        let Val::Const(inner) = *self.world.pool.val(v) else { unreachable!("constant of Const") };
        let size = match self.world.pool.val(inner) {
            Val::ByteString(b) => b.len() + 1,
            Val::ArrayValue(ty, vs) => layout::size_of(&self.world.pool, *ty) as usize * vs.len(),
            _ => WORD as usize,
        };
        let addr = self.heap.alloc(size);
        self.write_const(addr, inner)?;
        self.funcs.consts.insert(v, addr);
        Ok(addr)
    }

    fn write_const(&mut self, addr: usize, v: ValId) -> Flow<()> {
        match self.world.pool.val(v).clone() {
            Val::ByteString(bytes) => {
                for (i, b) in bytes.iter().enumerate() {
                    // SAFETY: `addr` holds `bytes.len() + 1` zeroed bytes.
                    unsafe { heap::store(addr + i, *b) };
                }
            }
            Val::ArrayValue(ty, vs) => {
                let size = layout::size_of(&self.world.pool, ty) as usize;
                for (i, &e) in vs.iter().enumerate() {
                    let value = self.val(e, &[])?;
                    self.store_ty(addr + i * size, ty, value);
                }
            }
            other => {
                let ty = self.val_ty(v);
                let ty = TypeId(self.world.pool.types.intern(ty));
                let value = self.val(v, &[])?;
                if matches!(other, Val::StructValue(_)) {
                    return Err(VmError::Unsupported("struct constant".into()).into());
                }
                self.store_ty(addr, ty, value);
            }
        }
        Ok(())
    }
}
