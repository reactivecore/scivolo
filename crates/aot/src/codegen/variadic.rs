//! Calls of variadic C functions, which Cranelift cannot make: each goes through a C shim with
//! the call's fixed argument types, compiled with the executable.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, Value, types};
use cranelift_module::{FuncId, Linkage, Module};
use nir::{GlobalId, Type, TypeId, ValId};

fn c_type(t: Option<types::Type>) -> &'static str {
    match t {
        None => "void",
        Some(types::I8) => "int8_t",
        Some(types::I16) => "int16_t",
        Some(types::I32) => "int32_t",
        Some(types::F32) => "float",
        Some(types::F64) => "double",
        Some(_) => "int64_t",
    }
}

impl FnBuilder<'_, '_> {
    /// Calls extern `g` of variadic type `ty` with `args` through a shim.
    pub fn variadic_call(&mut self, g: GlobalId, ty: TypeId, args: &[ValId]) -> R<Option<Value>> {
        let pool = &self.reach.world.pool;
        let Type::Function(params, ret) = pool.ty(ty).clone() else {
            return Err("not a function type".into());
        };
        let name = super::decls::extern_name(pool, g).ok_or("a variadic call of a non-extern")?;
        let fixed = params.len() - 1;
        let ret = match pool.ty(ret) {
            Type::Unit | Type::Nothing => None,
            _ => Some(self.ty(ret)?),
        };
        let mut arg_types = Vec::with_capacity(args.len());
        for (i, &a) in args.iter().enumerate() {
            let t = if i < fixed { params[i] } else { self.type_of(a)? };
            arg_types.push(self.ty(t)?);
        }
        let f = self.shim(&name, fixed, &arg_types, ret)?;
        let mut values = Vec::with_capacity(args.len());
        for (&a, &t) in args.iter().zip(&arg_types) {
            values.push(self.arg(a, t)?);
        }
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &values);
        Ok(self.b.inst_results(call).first().copied())
    }

    /// The shim calling `name`, whose first `fixed` parameters are fixed, with `args`.
    fn shim(
        &mut self,
        name: &str,
        fixed: usize,
        args: &[types::Type],
        ret: Option<types::Type>,
    ) -> R<FuncId> {
        let n = self.decls.shims.len();
        let shim = format!("scalai_aot_variadic_{n}");
        let params: Vec<String> =
            args.iter().enumerate().map(|(i, &t)| format!("{} a{i}", c_type(Some(t)))).collect();
        let fixed_types: Vec<&str> = args[..fixed].iter().map(|&t| c_type(Some(t))).collect();
        let names: Vec<String> = (0..args.len()).map(|i| format!("a{i}")).collect();
        let (r, ret_c) = (if ret.is_some() { "return " } else { "" }, c_type(ret));
        self.decls.shims.push(format!(
            "{ret_c} {name}({}, ...);\n{ret_c} {shim}({}) {{ {r}{name}({}); }}\n",
            fixed_types.join(", "),
            params.join(", "),
            names.join(", ")
        ));
        let mut sig = Signature::new(self.decls.call_conv);
        sig.params.extend(args.iter().map(|&t| AbiParam::new(t)));
        sig.returns.extend(ret.map(AbiParam::new));
        self.module.declare_function(&shim, Linkage::Import, &sig).map_err(|e| e.to_string())
    }
}
