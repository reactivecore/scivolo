//! `box` and `unbox`, which call Scala Native's box helpers.

use crate::interp::{Flow, FuncId};
use crate::{Vm, VmError};
use nir::{Type, TypeId, ValId};

impl Vm {
    pub fn boxing(&mut self, ty: TypeId, obj: ValId, slots: &[u64], boxing: bool) -> Flow {
        let v = self.val(obj, slots)?;
        let (module, f) = match self.funcs.boxers.get(&(ty, boxing)) {
            Some(&cached) => cached,
            None => {
                let helper = self.box_helper(ty, boxing)?;
                self.funcs.boxers.insert((ty, boxing), helper);
                helper
            }
        };
        self.call(f, &[module, v])
    }

    fn box_helper(&mut self, ty: TypeId, boxing: bool) -> Flow<(u64, FuncId)> {
        let Type::Ref { name, .. } = *self.world.pool.ty(ty) else {
            return Err(VmError::Unsupported("box into a non-class type".into()).into());
        };
        let class = self.world.pool.top_name(name).to_owned();
        let (module, sig) =
            helper(&class, boxing).ok_or_else(|| VmError::Unsupported(format!("box {class}")))?;
        let owner = self.world.top(module);
        let module = self.module(owner)?;
        let g = self.world.member(owner, &sig);
        Ok((module as u64, self.func(g)?))
    }
}

/// The module and mangled method that box or unbox values of box class `class`.
fn helper(class: &str, boxing: bool) -> Option<(&'static str, String)> {
    let (module, prim, short, unboxed) = match class {
        "java.lang.Boolean" => ("scala.runtime.BoxesRunTime$", "z", "Boolean", "Boolean"),
        "java.lang.Character" => ("scala.runtime.BoxesRunTime$", "c", "Character", "Char"),
        "java.lang.Byte" => ("scala.runtime.BoxesRunTime$", "b", "Byte", "Byte"),
        "java.lang.Short" => ("scala.runtime.BoxesRunTime$", "s", "Short", "Short"),
        "java.lang.Integer" => ("scala.runtime.BoxesRunTime$", "i", "Integer", "Int"),
        "java.lang.Long" => ("scala.runtime.BoxesRunTime$", "j", "Long", "Long"),
        "java.lang.Float" => ("scala.runtime.BoxesRunTime$", "f", "Float", "Float"),
        "java.lang.Double" => ("scala.runtime.BoxesRunTime$", "d", "Double", "Double"),
        other => {
            let short = other.rsplit('.').next()?;
            let prim = match short {
                "UByte" => "b",
                "UShort" => "s",
                "UInt" => "i",
                "ULong" => "j",
                "USize" | "Size" => "w",
                s if s == "Ptr" || s == "CArray" || s == "CVarArgList" || s.starts_with("CFuncPtr") => "R_",
                _ => return None,
            };
            ("scala.scalanative.runtime.Boxes$", prim, short, short)
        }
    };
    let (name, tys) = match boxing {
        true => (format!("boxTo{short}"), format!("{prim}L{}{class}", class.len())),
        false => (format!("unboxTo{unboxed}"), format!("L16java.lang.Object{prim}")),
    };
    Some((module, format!("D{}{name}{tys}EO", name.len())))
}
