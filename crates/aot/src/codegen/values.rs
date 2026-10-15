//! NIR values as Cranelift values.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use nir::{Type, TypeId, Val, ValId};

impl FnBuilder<'_, '_> {
    /// The NIR type of value `v`.
    pub fn type_of(&self, v: ValId) -> R<TypeId> {
        match self.reach.world.pool.val(v) {
            Val::Local(_, ty) | Val::Global(_, ty) => Ok(*ty),
            v => Err(format!("the type of {v:?}")),
        }
    }

    pub fn val(&mut self, v: ValId) -> R<Value> {
        let val = self.reach.world.pool.val(v).clone();
        let int = |b: &mut FunctionBuilder, t, x: i64| b.ins().iconst(t, x);
        Ok(match val {
            Val::Local(l, ty) => match self.vals.get(&l.0) {
                Some(&v) => v,
                None if matches!(self.reach.world.pool.ty(ty), Type::Unit) => int(&mut self.b, types::I8, 0),
                None => return Err(format!("undefined local %{}", l.0)),
            },
            Val::True => int(&mut self.b, types::I8, 1),
            Val::False | Val::Unit => int(&mut self.b, types::I8, 0),
            Val::Null => int(&mut self.b, self.decls.ptr, 0),
            Val::Zero(ty) => match self.ty(ty)? {
                types::F32 => self.b.ins().f32const(0.0),
                types::F64 => self.b.ins().f64const(0.0),
                t => int(&mut self.b, t, 0),
            },
            Val::Char(c) => int(&mut self.b, types::I16, c as i64),
            Val::Byte(x) => int(&mut self.b, types::I8, x as u8 as i64),
            Val::Short(x) => int(&mut self.b, types::I16, x as u16 as i64),
            Val::Int(x) => int(&mut self.b, types::I32, x as u32 as i64),
            Val::Long(x) => int(&mut self.b, types::I64, x),
            Val::Size(x) => int(&mut self.b, self.decls.ptr, x as i64),
            Val::Float(bits) => self.b.ins().f32const(Ieee32::with_bits(bits)),
            Val::Double(bits) => self.b.ins().f64const(Ieee64::with_bits(bits)),
            Val::ClassOf(g) => {
                let cls = self.reach.link(g)?;
                self.rtti_addr(cls)?
            }
            Val::Global(g, _) => {
                let defn = self.reach.world.defn(g).map_err(|e| e.to_string())?;
                if matches!(defn.kind, nir::DefnKind::Var { .. } | nir::DefnKind::Const { .. }) {
                    let data = self.decls.global_data(self.module, self.reach, g)?;
                    let gv = self.module.declare_data_in_func(data, self.b.func);
                    self.b.ins().symbol_value(self.decls.ptr, gv)
                } else {
                    let f = self.decls.func(self.module, self.reach, g)?;
                    let fref = self.module.declare_func_in_func(f, self.b.func);
                    self.b.ins().func_addr(self.decls.ptr, fref)
                }
            }
            Val::String(s) => {
                let pool = &self.reach.world.pool;
                let units: Vec<u16> = match pool.utf16.get(&s) {
                    Some(units) => units.to_vec(),
                    None => pool.str(s).encode_utf16().collect(),
                };
                let data = self.decls.string(self.module, self.reach, &units)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
            Val::Const(inner) => {
                let data = self.decls.constant(self.module, self.reach, inner)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
            // Temporary until #113 aot 3 (static fields, aggregate values).
            v => return Err(format!("value not supported yet: {v:?}")),
        })
    }

    /// `v` where a value of type `t` is expected: `unit` and `null` become zero of type `t`.
    pub fn arg(&mut self, v: ValId, t: types::Type) -> R<Value> {
        let pool = &self.reach.world.pool;
        let zero = match pool.val(v) {
            Val::Unit | Val::Null => true,
            Val::Local(_, ty) => matches!(pool.ty(*ty), Type::Unit),
            _ => false,
        };
        match zero {
            true => Ok(self.b.ins().iconst(t, 0)),
            false => self.val(v),
        }
    }
}
