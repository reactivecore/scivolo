//! NIR definitions → Cranelift IR → one object file, for the functions reachable from the entry.

mod body;
mod ops;

use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, isa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use nir::{Defn, DefnKind, GlobalId, Pool, Type, TypeId};
use std::collections::HashMap;

pub type R<T> = Result<T, String>;

/// The program's definitions and the symbols declared for them.
pub struct Symbols<'p> {
    pool: &'p Pool,
    defns: &'p [Defn],
    call_conv: CallConv,
    by_name: HashMap<GlobalId, usize>,
    by_member: HashMap<(&'p str, &'p str), usize>,
    funcs: HashMap<usize, FuncId>,
    modules: HashMap<GlobalId, DataId>,
    queue: Vec<usize>,
}

pub fn compile(pool: &Pool, defns: &[Defn], entry: &str) -> R<Vec<u8>> {
    let mut flags = settings::builder();
    let set = |f: &mut settings::Builder, k, v| f.set(k, v).map_err(|e| e.to_string());
    set(&mut flags, "opt_level", "speed")?;
    set(&mut flags, "is_pic", "true")?;
    set(&mut flags, "preserve_frame_pointers", "true")?;
    let isa = isa::lookup(target_lexicon::Triple::host())
        .map_err(|e| e.to_string())?
        .finish(settings::Flags::new(flags))
        .map_err(|e| e.to_string())?;
    let call_conv = isa.default_call_conv();
    let builder =
        ObjectBuilder::new(isa, "scalai-aot", default_libcall_names()).map_err(|e| e.to_string())?;
    let mut module = ObjectModule::new(builder);
    let mut sym = Symbols {
        pool,
        defns,
        call_conv,
        by_name: HashMap::new(),
        by_member: HashMap::new(),
        funcs: HashMap::new(),
        modules: HashMap::new(),
        queue: Vec::new(),
    };
    for (i, d) in defns.iter().enumerate() {
        if matches!(d.kind, DefnKind::Define { .. }) {
            sym.by_name.insert(d.name, i);
            if let Some(sig) = pool.member_sig(d.name) {
                sym.by_member.insert((pool.top_name(d.name), sig), i);
            }
        }
    }
    let target = sym.entry(entry)?;
    let target = sym.func(&mut module, target)?;
    entry_wrapper(&mut module, &sym, target)?;
    let mut ctx = Context::new();
    let mut fb = FunctionBuilderContext::new();
    while let Some(i) = sym.queue.pop() {
        let d = &defns[i];
        let DefnKind::Define { ty, body } = &d.kind else { unreachable!("queued a non-function") };
        ctx.func.signature = sym.signature(*ty)?;
        let mut f = body::FnBuilder::new(FunctionBuilder::new(&mut ctx.func, &mut fb), &mut module, &mut sym);
        f.body(body).map_err(|e| format!("{}: {e}", nir::show::global(pool, d.name, true)))?;
        f.finish();
        module.define_function(sym.funcs[&i], &mut ctx).map_err(|e| format!("{e:?}"))?;
        module.clear_context(&mut ctx);
    }
    module.finish().emit().map_err(|e| e.to_string())
}

/// `scalai_entry(n: i32) -> i32`, which calls `target`.
fn entry_wrapper(module: &mut ObjectModule, sym: &Symbols, target: FuncId) -> R<()> {
    let mut sig = Signature::new(sym.call_conv);
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let id = module.declare_function("scalai_entry", Linkage::Export, &sig).map_err(|e| e.to_string())?;
    let mut ctx = Context::new();
    ctx.func.signature = sig;
    let mut fb = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb);
    let block = b.create_block();
    b.append_block_params_for_function_params(block);
    b.switch_to_block(block);
    let callee = module.declare_func_in_func(target, b.func);
    let n = b.block_params(block)[0];
    let call = b.ins().call(callee, &[n]);
    let r = b.inst_results(call)[0];
    b.ins().return_(&[r]);
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(id, &mut ctx).map_err(|e| format!("{e:?}"))
}

impl<'p> Symbols<'p> {
    /// The `(Int) => Int` function whose method name is `name`.
    fn entry(&self, name: &str) -> R<usize> {
        let prefix = format!("D{}{name}", name.len());
        let found: Vec<usize> = (0..self.defns.len())
            .filter(|&i| {
                let d = &self.defns[i];
                let DefnKind::Define { ty, .. } = d.kind else { return false };
                let int = |t: &TypeId| matches!(self.pool.ty(*t), Type::Int);
                self.pool.member_sig(d.name).is_some_and(|s| s.starts_with(&prefix))
                    && matches!(self.pool.ty(ty), Type::Function(ps, r) if ps.len() == 1 && int(&ps[0]) && int(r))
            })
            .collect();
        match found[..] {
            [i] => Ok(i),
            [] => Err(format!("no method `{name}` of type (Int) => Int")),
            _ => {
                let names: Vec<String> =
                    found.iter().map(|&i| nir::show::global(self.pool, self.defns[i].name, true)).collect();
                Err(format!("several methods `{name}` of type (Int) => Int: {}", names.join(", ")))
            }
        }
    }

    /// The function of definition `i`, declared and queued for definition on first use.
    fn func(&mut self, module: &mut ObjectModule, i: usize) -> R<FuncId> {
        if let Some(&id) = self.funcs.get(&i) {
            return Ok(id);
        }
        let d = &self.defns[i];
        let DefnKind::Define { ty, .. } = d.kind else { return Err("not a function".into()) };
        let sig = self.signature(ty)?;
        let name = format!("_S{}", self.pool.mangle(d.name));
        let id = module.declare_function(&name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        self.funcs.insert(i, id);
        self.queue.push(i);
        Ok(id)
    }

    /// The instance of module `g`, which must have no fields: a zeroed slot whose constructor
    /// does not run.
    // Temporary until #113 aot 2 (module load functions): modules are allocated and constructed.
    fn module(&mut self, module: &mut ObjectModule, g: GlobalId) -> R<DataId> {
        if let Some(&id) = self.modules.get(&g) {
            return Ok(id);
        }
        let pool = self.pool;
        let field = |d: &&Defn| {
            matches!(d.kind, DefnKind::Var { .. })
                && matches!(pool.global(d.name), nir::Global::Member(owner, _) if *owner == g)
        };
        if self.defns.iter().any(|d| field(&d)) {
            return Err(format!("module {} has fields: not supported yet", pool.top_name(g)));
        }
        let name = format!("_S{}.instance", self.pool.mangle(g));
        let id = module.declare_data(&name, Linkage::Local, true, false).map_err(|e| e.to_string())?;
        let mut desc = DataDescription::new();
        desc.define_zeroinit(16);
        module.define_data(id, &desc).map_err(|e| e.to_string())?;
        self.modules.insert(g, id);
        Ok(id)
    }

    fn signature(&self, ty: TypeId) -> R<Signature> {
        let Type::Function(params, ret) = self.pool.ty(ty) else { return Err("not a function type".into()) };
        let mut sig = Signature::new(self.call_conv);
        for &p in params {
            sig.params.push(AbiParam::new(value_type(self.pool, p)?));
        }
        if !matches!(self.pool.ty(*ret), Type::Unit | Type::Nothing) {
            sig.returns.push(AbiParam::new(value_type(self.pool, *ret)?));
        }
        Ok(sig)
    }
}

fn value_type(pool: &Pool, t: TypeId) -> R<types::Type> {
    Ok(match pool.ty(t) {
        Type::Bool | Type::Byte | Type::Unit | Type::Nothing => types::I8,
        Type::Char | Type::Short => types::I16,
        Type::Int => types::I32,
        Type::Long | Type::Size | Type::Ptr | Type::Null | Type::Virtual => types::I64,
        Type::Array { .. } | Type::Ref { .. } | Type::Var(_) | Type::Function(..) => types::I64,
        Type::Float => types::F32,
        Type::Double => types::F64,
        t => return Err(format!("value type {t:?}")),
    })
}
