//! Calls of variadic C functions, which Cranelift cannot make: each goes through a C shim with
//! the call's fixed argument types, compiled with the executable.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Signature, Value};
use cranelift_module::{FuncId, Linkage, Module};
use nir::{GlobalId, Pool, Type, TypeId, ValId};

/// The C type of NIR type `t`; `None` is `void`.
fn c_type(pool: &Pool, t: Option<TypeId>) -> &'static str {
    match t.map(|t| pool.ty(t)) {
        None => "void",
        Some(Type::Bool) => "uint8_t",
        Some(Type::Byte) => "int8_t",
        Some(Type::Short) => "int16_t",
        Some(Type::Char) => "uint16_t",
        Some(Type::Int) => "int32_t",
        Some(Type::Long) => "int64_t",
        Some(Type::Float) => "float",
        Some(Type::Double) => "double",
        Some(_) => "void *",
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
        if fixed == 0 {
            return Err(format!("{name} has no fixed parameter"));
        }
        let ret = (!matches!(pool.ty(ret), Type::Unit | Type::Nothing)).then_some(ret);
        let mut arg_types = Vec::with_capacity(args.len());
        for (i, &a) in args.iter().enumerate() {
            arg_types.push(if i < fixed { params[i] } else { self.type_of(a)? });
        }
        let f = self.shim(&name, fixed, &arg_types, ret)?;
        let mut values = Vec::with_capacity(args.len());
        for (&a, &t) in args.iter().zip(&arg_types) {
            let t = self.ty(t)?;
            values.push(self.arg(a, t)?);
        }
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &values);
        Ok(self.b.inst_results(call).first().copied())
    }

    /// The shim calling `name`, whose first `fixed` parameters are fixed, with `args`.
    fn shim(&mut self, name: &str, fixed: usize, args: &[TypeId], ret: Option<TypeId>) -> R<FuncId> {
        let pool = &self.reach.world.pool;
        let n = self.decls.shims.len();
        let shim = format!("scalai_aot_variadic_{n}");
        let params: Vec<String> =
            args.iter().enumerate().map(|(i, &t)| format!("{} a{i}", c_type(pool, Some(t)))).collect();
        let fixed_types: Vec<&str> = args[..fixed].iter().map(|&t| c_type(pool, Some(t))).collect();
        let names: Vec<String> = (0..args.len()).map(|i| format!("a{i}")).collect();
        let (r, ret_c) = (if ret.is_some() { "return " } else { "" }, c_type(pool, ret));
        let source = format!(
            "{ret_c} {name}({}, ...);\n{ret_c} {shim}({}) {{ {r}{name}({}); }}\n",
            fixed_types.join(", "),
            params.join(", "),
            names.join(", ")
        );
        let mut sig = Signature::new(self.decls.call_conv);
        for &t in args {
            sig.params.push(self.decls.abi_param(pool, t)?);
        }
        if let Some(t) = ret {
            sig.returns.push(self.decls.abi_param(pool, t)?);
        }
        self.decls.shims.push(source);
        self.module.declare_function(&shim, Linkage::Import, &sig).map_err(|e| e.to_string())
    }
}
