//! NIR definitions → Cranelift IR → object code. Runtime operations (module loading, allocation,
//! virtual dispatch, type tests, boxing, throwing) become calls to `rt_*` stubs; field offsets are
//! fake; exceptions are a global flag checked after every call (`eh_flag`).

use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, GlobalValue, InstBuilder, MemFlagsData, Signature, StackSlotData,
    StackSlotKind, TrapCode, Value, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{isa, isa::CallConv};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use nir::{Bin, Body, Comp, Cond, Conv, Defn, DefnKind, GlobalId, Inst, Next, Op, Pool, StrId, Type, TypeId, Val, ValId};
use std::collections::HashMap;

type R<T> = Result<T, String>;

#[derive(Default)]
pub struct Stats {
    pub functions: usize,
    pub bailed: usize,
    pub insts: usize,
    pub bail_reasons: HashMap<String, usize>,
}

struct Shared<'p> {
    pool: &'p Pool,
    call_conv: CallConv,
    funcs: HashMap<GlobalId, FuncId>,
    datas: HashMap<GlobalId, DataId>,
    stubs: HashMap<&'static str, FuncId>,
    strings: HashMap<StrId, DataId>,
    exc: DataId,
    anon: usize,
}

pub struct Translator<'p> {
    module: ObjectModule,
    ctx: Context,
    fb: FunctionBuilderContext,
    shared: Shared<'p>,
    eh_flag: bool,
    pub disasm: bool,
}

/// The object writer applies the platform's C mangling (a `_` prefix on Mach-O) itself.
fn c_sym(name: &str) -> String {
    name.to_string()
}

fn cl_type(pool: &Pool, t: TypeId) -> R<types::Type> {
    Ok(match pool.ty(t) {
        Type::Bool | Type::Byte | Type::Unit | Type::Nothing => types::I8,
        Type::Char | Type::Short => types::I16,
        Type::Int => types::I32,
        Type::Long | Type::Size | Type::Ptr | Type::Null | Type::Virtual => types::I64,
        Type::Array { .. } | Type::Ref { .. } | Type::Var(_) => types::I64,
        Type::Float => types::F32,
        Type::Double => types::F64,
        Type::Function(..) => types::I64,
        t => return Err(format!("value type {t:?}")),
    })
}

fn size_align(pool: &Pool, t: TypeId) -> R<(u32, u32)> {
    Ok(match pool.ty(t) {
        Type::Bool | Type::Byte | Type::Unit | Type::Nothing => (1, 1),
        Type::Char | Type::Short => (2, 2),
        Type::Int | Type::Float => (4, 4),
        Type::Int128 => (16, 16),
        Type::ArrayValue(elem, n) => {
            let (s, a) = size_align(pool, *elem)?;
            (s * n, a)
        }
        Type::StructValue(fields) => {
            let (mut off, mut align): (u32, u32) = (0, 1);
            for &f in fields {
                let (s, a) = size_align(pool, f)?;
                off = off.div_ceil(a) * a + s;
                align = align.max(a);
            }
            (off.div_ceil(align) * align, align)
        }
        Type::Vararg => return Err("vararg size".into()),
        _ => (8, 8),
    })
}

fn field_offset(pool: &Pool, t: TypeId, index: i32) -> R<(u32, TypeId)> {
    match pool.ty(t) {
        Type::StructValue(fields) => {
            let mut off: u32 = 0;
            for (i, &f) in fields.iter().enumerate() {
                let (s, a) = size_align(pool, f)?;
                off = off.div_ceil(a) * a;
                if i as i32 == index {
                    return Ok((off, f));
                }
                off += s;
            }
            Err("struct index".into())
        }
        Type::ArrayValue(elem, _) => Ok((size_align(pool, *elem)?.0 * index as u32, *elem)),
        t => Err(format!("elem into {t:?}")),
    }
}

impl<'p> Translator<'p> {
    pub fn new(pool: &'p Pool, opt: &str, eh_flag: bool, verify: bool) -> Self {
        let mut flags = settings::builder();
        flags.set("opt_level", opt).unwrap();
        flags.set("is_pic", "true").unwrap();
        flags.set("preserve_frame_pointers", "true").unwrap();
        flags.set("enable_verifier", if verify { "true" } else { "false" }).unwrap();
        let isa = isa::lookup(target_lexicon::Triple::host()).unwrap().finish(settings::Flags::new(flags)).unwrap();
        let call_conv = isa.default_call_conv();
        let builder = ObjectBuilder::new(isa, "aot", default_libcall_names()).unwrap();
        let mut module = ObjectModule::new(builder);
        let exc = module.declare_data(&c_sym("rt_exc"), Linkage::Import, true, false).unwrap();
        let shared = Shared {
            pool,
            call_conv,
            funcs: HashMap::new(),
            datas: HashMap::new(),
            stubs: HashMap::new(),
            strings: HashMap::new(),
            exc,
            anon: 0,
        };
        Self { module, ctx: Context::new(), fb: FunctionBuilderContext::new(), shared, eh_flag, disasm: false }
    }

    pub fn declare_all(&mut self, defns: &[Defn]) {
        for d in defns {
            match &d.kind {
                DefnKind::Define { ty, .. } => {
                    let Ok(sig) = self.shared.signature(*ty) else { continue };
                    let name = self.shared.func_name(d.name);
                    if let Ok(id) = self.module.declare_function(&name, Linkage::Export, &sig) {
                        self.shared.funcs.insert(d.name, id);
                    }
                }
                DefnKind::Var { ty, .. } | DefnKind::Const { ty, .. } => {
                    let size = size_align(self.shared.pool, *ty).map(|s| s.0).unwrap_or(8);
                    self.define_data(d.name, size as usize);
                }
                DefnKind::Class { .. } | DefnKind::Module { .. } | DefnKind::Trait { .. } => {
                    self.define_data(d.name, 128);
                }
                DefnKind::Declare { .. } => {}
            }
        }
    }

    fn define_data(&mut self, g: GlobalId, size: usize) {
        let name = self.shared.pool.mangle(g);
        let Ok(id) = self.module.declare_data(&name, Linkage::Export, true, false) else { return };
        if self.shared.datas.insert(g, id).is_none() {
            let mut desc = DataDescription::new();
            desc.define_zeroinit(size);
            let _ = self.module.define_data(id, &desc);
        }
    }

    pub fn define_all(&mut self, defns: &[Defn]) -> Stats {
        let mut stats = Stats::default();
        for d in defns {
            let DefnKind::Define { ty, body } = &d.kind else { continue };
            let Some(&fid) = self.shared.funcs.get(&d.name) else { continue };
            stats.functions += 1;
            stats.insts += body.insts.len();
            if self.disasm {
                eprintln!("== {}", self.shared.func_name(d.name));
            }
            let result = self.define(fid, *ty, body);
            self.module.clear_context(&mut self.ctx);
            if let Err(e) = result {
                self.fb = FunctionBuilderContext::new();
                if stats.bailed < 5 {
                    eprintln!("bail {}: {e}", self.shared.pool.mangle(d.name));
                }
                stats.bailed += 1;
                let key = e.split(':').next().unwrap_or("").to_string();
                *stats.bail_reasons.entry(key).or_default() += 1;
            }
        }
        stats
    }

    fn define(&mut self, fid: FuncId, ty: TypeId, body: &Body) -> R<()> {
        let sig = self.shared.signature(ty)?;
        self.ctx.func.signature = sig.clone();
        let cfg = self.module.target_config();
        let b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fb);
        let flags = MemFlagsData::new();
        let mut f = FnTranslator {
            b,
            module: &mut self.module,
            sh: &mut self.shared,
            sig,
            flags,
            eh_flag: self.eh_flag,
            blocks: HashMap::new(),
            vals: HashMap::new(),
            var_types: HashMap::new(),
            local_module: HashMap::new(),
            local_func: HashMap::new(),
            exc_addr: None,
            propagate: None,
            throw_blocks: HashMap::new(),
            pending: Vec::new(),
        };
        f.body(body)?;
        f.emit_slow_paths();
        f.b.seal_all_blocks();
        f.b.finalize(cfg);
        self.ctx.want_disasm = self.disasm;
        self.module.define_function(fid, &mut self.ctx).map_err(|e| format!("define: {e}"))?;
        if self.disasm {
            let code = self.ctx.compiled_code().unwrap();
            eprintln!("{}\n{}", self.ctx.func.display(), code.vcode.as_deref().unwrap_or(""));
        }
        Ok(())
    }

    /// Defines `rt_selftest(n) = n + 1` to check the object pipeline independently of NIR.
    pub fn selftest(&mut self) {
        let mut sig = Signature::new(self.shared.call_conv);
        sig.params.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));
        let fid = self.module.declare_function(&c_sym("rt_selftest"), Linkage::Export, &sig).unwrap();
        self.ctx.func.signature = sig;
        let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fb);
        let block = b.create_block();
        b.append_block_params_for_function_params(block);
        b.switch_to_block(block);
        let n = b.block_params(block)[0];
        let r = b.ins().iadd_imm_s(n, 1);
        b.ins().return_(&[r]);
        b.seal_all_blocks();
        b.finalize(self.module.target_config());
        self.module.define_function(fid, &mut self.ctx).unwrap();
        self.module.clear_context(&mut self.ctx);
    }

    pub fn finish(self) -> Vec<u8> {
        self.module.finish().emit().unwrap()
    }
}

impl<'p> Shared<'p> {
    fn func_name(&self, g: GlobalId) -> String {
        format!("_S{}", self.pool.mangle(g))
    }

    fn signature(&self, ty: TypeId) -> R<Signature> {
        let Type::Function(params, ret) = self.pool.ty(ty) else {
            return Err(format!("not a function type: {:?}", self.pool.ty(ty)));
        };
        let mut sig = Signature::new(self.call_conv);
        for &p in params {
            sig.params.push(AbiParam::new(cl_type(self.pool, p)?));
        }
        if !matches!(self.pool.ty(*ret), Type::Unit | Type::Nothing) {
            sig.returns.push(AbiParam::new(cl_type(self.pool, *ret)?));
        }
        Ok(sig)
    }

    fn stub(&mut self, module: &mut ObjectModule, name: &'static str, params: &[types::Type], ret: Option<types::Type>) -> FuncId {
        if let Some(&id) = self.stubs.get(name) {
            return id;
        }
        let mut sig = Signature::new(self.call_conv);
        sig.params.extend(params.iter().map(|&t| AbiParam::new(t)));
        sig.returns.extend(ret.map(AbiParam::new));
        let id = module.declare_function(&c_sym(name), Linkage::Import, &sig).unwrap();
        self.stubs.insert(name, id);
        id
    }

    fn func(&mut self, module: &mut ObjectModule, g: GlobalId, ty: TypeId) -> R<FuncId> {
        if let Some(&id) = self.funcs.get(&g) {
            return Ok(id);
        }
        let sig = self.signature(ty)?;
        let id = module.declare_function(&self.func_name(g), Linkage::Import, &sig).map_err(|e| format!("declare: {e}"))?;
        self.funcs.insert(g, id);
        Ok(id)
    }

    fn data(&mut self, module: &mut ObjectModule, g: GlobalId) -> DataId {
        if let Some(&id) = self.datas.get(&g) {
            return id;
        }
        let id = module.declare_data(&self.pool.mangle(g), Linkage::Import, true, false).unwrap();
        self.datas.insert(g, id);
        id
    }

    fn bytes(&mut self, module: &mut ObjectModule, bytes: Vec<u8>) -> DataId {
        self.anon += 1;
        let id = module.declare_data(&format!("rt_const_{}", self.anon), Linkage::Local, false, false).unwrap();
        let mut desc = DataDescription::new();
        desc.define(bytes.into_boxed_slice());
        module.define_data(id, &desc).unwrap();
        id
    }
}

struct FnTranslator<'a, 'p> {
    b: FunctionBuilder<'a>,
    module: &'a mut ObjectModule,
    sh: &'a mut Shared<'p>,
    sig: Signature,
    flags: MemFlagsData,
    eh_flag: bool,
    blocks: HashMap<u64, Block>,
    vals: HashMap<u64, Value>,
    var_types: HashMap<u64, TypeId>,
    local_module: HashMap<u64, GlobalId>,
    local_func: HashMap<u64, FuncId>,
    exc_addr: Option<Value>,
    propagate: Option<Block>,
    throw_blocks: HashMap<&'static str, Block>,
    pending: Vec<(&'static str, Block, Vec<types::Type>)>,
}

impl<'a, 'p> FnTranslator<'a, 'p> {
    fn pool(&self) -> &'p Pool {
        self.sh.pool
    }

    fn body(&mut self, body: &Body) -> R<()> {
        let mut first = true;
        for inst in &body.insts {
            if let Inst::Label(l, params) = inst {
                let block = self.b.create_block();
                if first {
                    self.b.append_block_params_for_function_params(block);
                    first = false;
                } else {
                    for p in params {
                        self.b.append_block_param(block, cl_type(self.pool(), p.ty)?);
                    }
                }
                self.blocks.insert(l.0, block);
            }
        }
        let mut open = false;
        for inst in &body.insts {
            match inst {
                Inst::Label(l, params) => {
                    let block = self.blocks[&l.0];
                    self.b.switch_to_block(block);
                    let bp = self.b.block_params(block).to_vec();
                    for (p, v) in params.iter().zip(bp) {
                        self.vals.insert(p.id.0, v);
                    }
                    open = true;
                }
                _ => {
                    if !open {
                        let block = self.b.create_block();
                        self.b.switch_to_block(block);
                        open = true;
                    }
                    if self.inst(inst)? {
                        open = false;
                    }
                }
            }
        }
        Ok(())
    }

    fn val(&mut self, v: ValId) -> R<Value> {
        let pool = self.pool();
        Ok(match pool.val(v) {
            Val::Local(l, ty) => match self.vals.get(&l.0) {
                Some(&v) => v,
                None if matches!(pool.ty(*ty), Type::Unit) => self.b.ins().iconst(types::I8, 0),
                None => return Err(format!("undefined local: %{}", l.0)),
            },
            Val::True => self.b.ins().iconst(types::I8, 1),
            Val::False | Val::Unit => self.b.ins().iconst(types::I8, 0),
            Val::Null => self.b.ins().iconst(types::I64, 0),
            Val::Zero(ty) => match cl_type(pool, *ty)? {
                types::F32 => self.b.ins().f32const(0.0),
                types::F64 => self.b.ins().f64const(0.0),
                t => self.b.ins().iconst(t, 0),
            },
            Val::Char(c) => self.b.ins().iconst(types::I16, *c as i64),
            Val::Byte(x) => self.b.ins().iconst(types::I8, *x as i64),
            Val::Short(x) => self.b.ins().iconst(types::I16, *x as i64),
            Val::Int(x) => self.b.ins().iconst(types::I32, *x as i64),
            Val::Long(x) => self.b.ins().iconst(types::I64, *x),
            Val::Size(x) => self.b.ins().iconst(types::I64, *x as i64),
            Val::Float(bits) => self.b.ins().f32const(Ieee32::with_bits(*bits)),
            Val::Double(bits) => self.b.ins().f64const(Ieee64::with_bits(*bits)),
            Val::Global(g, _) => {
                if let Some(&fid) = self.sh.funcs.get(g) {
                    let fref = self.module.declare_func_in_func(fid, self.b.func);
                    self.b.ins().func_addr(types::I64, fref)
                } else {
                    let did = self.sh.data(self.module, *g);
                    self.data_addr(did)
                }
            }
            Val::ClassOf(g) => {
                let did = self.sh.data(self.module, *g);
                self.data_addr(did)
            }
            Val::String(s) => {
                let did = match self.sh.strings.get(s) {
                    Some(&id) => id,
                    None => {
                        let bytes: Vec<u8> = pool.str(*s).encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
                        let id = self.sh.bytes(self.module, bytes);
                        self.sh.strings.insert(*s, id);
                        id
                    }
                };
                self.data_addr(did)
            }
            Val::ByteString(bytes) => {
                let id = self.sh.bytes(self.module, bytes.clone());
                self.data_addr(id)
            }
            Val::Const(inner) => {
                let size = match pool.val(*inner) {
                    Val::StructValue(_) | Val::ArrayValue(..) => 64,
                    _ => 8,
                };
                let id = self.sh.bytes(self.module, vec![0; size]);
                self.data_addr(id)
            }
            v => return Err(format!("value: {v:?}")),
        })
    }

    fn data_addr(&mut self, did: DataId) -> Value {
        let gv: GlobalValue = self.module.declare_data_in_func(did, self.b.func);
        self.b.ins().symbol_value(types::I64, gv)
    }

    fn vals(&mut self, vs: &[ValId]) -> R<Vec<Value>> {
        vs.iter().map(|&v| self.val(v)).collect()
    }

    fn block_args(&mut self, vs: &[ValId]) -> R<Vec<BlockArg>> {
        Ok(self.vals(vs)?.into_iter().map(BlockArg::from).collect())
    }

    fn call_stub(&mut self, name: &'static str, params: &[types::Type], ret: Option<types::Type>, args: &[Value]) -> Option<Value> {
        let fid = self.sh.stub(self.module, name, params, ret);
        let fref = self.module.declare_func_in_func(fid, self.b.func);
        let call = self.b.ins().call(fref, args);
        self.b.inst_results(call).first().copied()
    }

    fn exc_addr(&mut self) -> Value {
        if let Some(v) = self.exc_addr {
            return v;
        }
        let v = self.data_addr(self.sh.exc);
        self.exc_addr = Some(v);
        v
    }

    /// The block that returns zero values after a throw propagates out of this function; its body
    /// is emitted by `emit_slow_paths`.
    fn propagate(&mut self) -> Block {
        if let Some(b) = self.propagate {
            return b;
        }
        let block = self.b.create_block();
        self.propagate = Some(block);
        block
    }

    fn emit_slow_paths(&mut self) {
        for (name, block, params) in std::mem::take(&mut self.pending) {
            self.b.switch_to_block(block);
            let args = self.b.block_params(block).to_vec();
            self.call_stub(name, &params, None, &args);
            if self.eh_flag {
                let propagate = self.propagate();
                self.b.ins().jump(propagate, &[]);
            } else {
                self.b.ins().trap(TrapCode::unwrap_user(2));
            }
        }
        if let Some(block) = self.propagate {
            self.b.switch_to_block(block);
            let rets: Vec<Value> = self.sig.returns.clone().iter().map(|r| self.zero(r.value_type)).collect();
            self.b.ins().return_(&rets);
        }
    }

    fn zero(&mut self, t: types::Type) -> Value {
        match t {
            types::F32 => self.b.ins().f32const(0.0),
            types::F64 => self.b.ins().f64const(0.0),
            t => self.b.ins().iconst(t, 0),
        }
    }

    /// Jumps to the unwind handler (or propagates) when the exception flag is set.
    fn check_exception(&mut self, unwind: &Next) -> R<()> {
        if !self.eh_flag {
            return Ok(());
        }
        let addr = self.exc_addr();
        let flags = self.flags;
        let e = self.b.ins().load(types::I64, flags, addr, 0);
        let cont = self.b.create_block();
        match unwind {
            Next::Unwind(param, to) => {
                let handler = self.b.create_block();
                self.b.ins().brif(e, handler, &[], cont, &[]);
                self.b.switch_to_block(handler);
                let zero = self.b.ins().iconst(types::I64, 0);
                self.b.ins().store(flags, zero, addr, 0);
                self.vals.insert(param.id.0, e);
                self.jump(to)?;
            }
            _ => {
                let propagate = self.propagate();
                self.b.ins().brif(e, propagate, &[], cont, &[]);
            }
        }
        self.b.switch_to_block(cont);
        Ok(())
    }

    fn throw_value(&mut self, exc: Value, unwind: &Next) -> R<()> {
        if !self.eh_flag {
            self.call_stub("rt_throw", &[types::I64], None, &[exc]);
            self.b.ins().trap(TrapCode::unwrap_user(1));
            return Ok(());
        }
        match unwind {
            Next::Unwind(param, to) => {
                self.vals.insert(param.id.0, exc);
                self.jump(to)
            }
            _ => {
                let addr = self.exc_addr();
                let flags = self.flags;
                self.b.ins().store(flags, exc, addr, 0);
                let propagate = self.propagate();
                self.b.ins().jump(propagate, &[]);
                Ok(())
            }
        }
    }

    /// A block that calls the throwing stub `name` and propagates, shared per function.
    fn throw_block(&mut self, name: &'static str, params: &[types::Type]) -> Block {
        if let Some(&b) = self.throw_blocks.get(name) {
            return b;
        }
        let block = self.b.create_block();
        for &t in params {
            self.b.append_block_param(block, t);
        }
        self.pending.push((name, block, params.to_vec()));
        self.throw_blocks.insert(name, block);
        block
    }

    fn null_check(&mut self, obj: Value) {
        let throw = self.throw_block("rt_throw_null", &[]);
        let cont = self.b.create_block();
        self.b.ins().brif(obj, cont, &[], throw, &[]);
        self.b.switch_to_block(cont);
    }

    fn jump(&mut self, next: &Next) -> R<()> {
        let Next::Label(l, args) = next else { return Err(format!("jump to {next:?}")) };
        let block = *self.blocks.get(&l.0).ok_or("unknown label")?;
        let args = self.block_args(args)?;
        self.b.ins().jump(block, &args);
        Ok(())
    }

    fn branch(&mut self, cond: Value, then: &Next, els: &Next) -> R<()> {
        let (Next::Label(tl, ta), Next::Label(el, ea)) = (then, els) else { return Err("branch targets".into()) };
        let (tb, eb) = (self.blocks[&tl.0], self.blocks[&el.0]);
        let ta = self.block_args(ta)?;
        let ea = self.block_args(ea)?;
        self.b.ins().brif(cond, tb, &ta, eb, &ea);
        Ok(())
    }

    fn linktime(&self, cond: &Cond) -> bool {
        match cond {
            Cond::Simple { property, comp, value, .. } => {
                let name = self.pool().str(*property);
                let truth = match name {
                    n if n.contains("isMac") => true,
                    n if n.contains("isMultithreadingEnabled") => false,
                    n if n.contains("is32BitPlatform") => false,
                    n if n.contains("isWindows") || n.contains("isLinux") || n.contains("isFreeBSD") => false,
                    _ => false,
                };
                let expected = matches!(self.pool().val(*value), Val::True);
                match comp {
                    Comp::Ieq => truth == expected,
                    Comp::Ine => truth != expected,
                    _ => true,
                }
            }
            Cond::Complex { bin, left, right, .. } => {
                let (l, r) = (self.linktime(left), self.linktime(right));
                match bin {
                    Bin::And => l && r,
                    Bin::Or => l || r,
                    _ => l,
                }
            }
        }
    }

    /// Translates one instruction; `true` when it ends the block.
    fn inst(&mut self, inst: &Inst) -> R<bool> {
        match inst {
            Inst::Label(..) => unreachable!(),
            Inst::Let { id, op, unwind, .. } => {
                if let Some(v) = self.op(id.0, op, unwind)? {
                    self.vals.insert(id.0, v);
                }
                Ok(false)
            }
            Inst::Ret(v) => {
                if self.sig.returns.is_empty() {
                    self.b.ins().return_(&[]);
                } else {
                    let v = self.val(*v)?;
                    self.b.ins().return_(&[v]);
                }
                Ok(true)
            }
            Inst::Jump(next) => self.jump(next).map(|_| true),
            Inst::If(c, then, els) => {
                let c = self.val(*c)?;
                self.branch(c, then, els).map(|_| true)
            }
            Inst::Switch(scrut, default, cases) => {
                let s = self.val(*scrut)?;
                for case in cases {
                    let Next::Case(cv, to) = case else { return Err("switch case".into()) };
                    let cv = self.val(*cv)?;
                    let eq = self.b.ins().icmp(IntCC::Equal, s, cv);
                    let next = self.b.create_block();
                    let Next::Label(l, args) = &**to else { return Err("case target".into()) };
                    let block = self.blocks[&l.0];
                    let args = self.block_args(args)?;
                    self.b.ins().brif(eq, block, &args, next, &[]);
                    self.b.switch_to_block(next);
                }
                self.jump(default).map(|_| true)
            }
            Inst::Throw(v, unwind) => {
                let v = self.val(*v)?;
                self.throw_value(v, unwind).map(|_| true)
            }
            Inst::Unreachable(_) => {
                self.b.ins().trap(TrapCode::unwrap_user(3));
                Ok(true)
            }
            Inst::LinktimeIf(cond, then, els) => {
                let next = if self.linktime(cond) { then } else { els };
                self.jump(next).map(|_| true)
            }
        }
    }

    fn fake_offset(&self, name: GlobalId) -> i32 {
        8 + (name.0 % 8) as i32 * 8
    }

    fn int_to_i64(&mut self, v: Value) -> Value {
        let flags = self.flags;
        match self.b.func.dfg.value_type(v) {
            types::I64 => v,
            types::F32 => {
                let i = self.b.ins().bitcast(types::I32, flags, v);
                self.b.ins().uextend(types::I64, i)
            }
            types::F64 => self.b.ins().bitcast(types::I64, flags, v),
            _ => self.b.ins().sextend(types::I64, v),
        }
    }

    fn unboxed_type(&self, ty: TypeId) -> types::Type {
        let Type::Ref { name, .. } = self.pool().ty(ty) else { return types::I64 };
        let n = self.pool().top_name(*name);
        match n.rsplit('.').next().unwrap_or("") {
            "Integer" | "UInt" => types::I32,
            "Long" | "ULong" => types::I64,
            "Boolean" | "Byte" | "UByte" => types::I8,
            "Character" | "Short" | "UShort" => types::I16,
            "Float" => types::F32,
            "Double" => types::F64,
            _ => types::I64,
        }
    }

    fn from_i64(&mut self, v: Value, t: types::Type) -> Value {
        let flags = self.flags;
        match t {
            types::I64 => v,
            types::F64 => self.b.ins().bitcast(types::F64, flags, v),
            types::F32 => {
                let i = self.b.ins().ireduce(types::I32, v);
                self.b.ins().bitcast(types::F32, flags, i)
            }
            t => self.b.ins().ireduce(t, v),
        }
    }

    fn array_elem_addr(&mut self, arr: Value, idx: Value, elem_size: u32) -> Value {
        self.null_check(arr);
        let flags = self.flags;
        let len = self.b.ins().load(types::I32, flags, arr, 8);
        let bad = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx, len);
        let throw = self.throw_block("rt_throw_bounds", &[types::I64, types::I64]);
        let cont = self.b.create_block();
        let idx64 = self.b.ins().sextend(types::I64, idx);
        let len64 = self.b.ins().sextend(types::I64, len);
        self.b.ins().brif(bad, throw, &[idx64.into(), len64.into()], cont, &[]);
        self.b.switch_to_block(cont);
        let off = self.b.ins().imul_imm_s(idx64, elem_size as i64);
        let base = self.b.ins().iadd_imm_s(arr, 16);
        self.b.ins().iadd(base, off)
    }

    fn op(&mut self, id: u64, op: &Op, unwind: &Next) -> R<Option<Value>> {
        let pool = self.pool();
        let flags = self.flags;
        Ok(Some(match op {
            Op::Call { ty, ptr, args } => {
                let sig = self.sh.signature(*ty)?;
                let args = self.vals(args)?;
                if args.len() != sig.params.len() {
                    return Err("call arity".into());
                }
                let direct = match pool.val(*ptr) {
                    Val::Global(g, _) => Some(self.sh.func(self.module, *g, *ty)?),
                    Val::Local(l, _) => self.local_func.get(&l.0).copied(),
                    _ => None,
                };
                let call = match direct {
                    Some(fid) => {
                        let fref = self.module.declare_func_in_func(fid, self.b.func);
                        self.b.ins().call(fref, &args)
                    }
                    None => {
                        let callee = self.val(*ptr)?;
                        let sigref = self.b.import_signature(sig.clone());
                        self.b.ins().call_indirect(sigref, callee, &args)
                    }
                };
                let result = self.b.inst_results(call).first().copied();
                self.check_exception(unwind)?;
                return Ok(result);
            }
            Op::Load { ty, ptr, .. } => {
                let p = self.val(*ptr)?;
                self.b.ins().load(cl_type(pool, *ty)?, flags, p, 0)
            }
            Op::Store { ptr, value, .. } => {
                let p = self.val(*ptr)?;
                let v = self.val(*value)?;
                self.b.ins().store(flags, v, p, 0);
                return Ok(None);
            }
            Op::Elem { ty, ptr, indexes } => {
                let p = self.val(*ptr)?;
                let (size, _) = size_align(pool, *ty)?;
                let i0 = self.val(indexes[0])?;
                let i0 = self.int_to_i64(i0);
                let off = self.b.ins().imul_imm_s(i0, size as i64);
                let mut addr = self.b.ins().iadd(p, off);
                let mut t = *ty;
                for &ix in &indexes[1..] {
                    let Val::Int(i) = pool.val(ix) else { return Err("elem index".into()) };
                    let (o, ft) = field_offset(pool, t, *i)?;
                    addr = self.b.ins().iadd_imm_s(addr, o as i64);
                    t = ft;
                }
                addr
            }
            Op::Extract { .. } | Op::Insert { .. } => return Err("aggregate".into()),
            Op::Stackalloc { ty, n } => {
                let (size, align) = size_align(pool, *ty)?;
                let count = match pool.val(*n) {
                    Val::Int(k) if *k >= 0 => Some(*k as u64),
                    Val::Long(k) if *k >= 0 => Some(*k as u64),
                    Val::Size(k) => Some(*k),
                    _ => None,
                };
                match count {
                    Some(k) => {
                        let bytes = size * (k as u32).max(1);
                        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            bytes,
                            align.trailing_zeros() as u8,
                        ));
                        self.b.ins().stack_addr(types::I64, slot, 0)
                    }
                    None => {
                        let n = self.val(*n)?;
                        let n = self.int_to_i64(n);
                        let bytes = self.b.ins().imul_imm_s(n, size as i64);
                        self.call_stub("rt_stackalloc", &[types::I64], Some(types::I64), &[bytes]).unwrap()
                    }
                }
            }
            Op::Bin { bin, ty, l, r } => {
                let (l, r) = (self.val(*l)?, self.val(*r)?);
                let t = cl_type(pool, *ty)?;
                match bin {
                    Bin::Iadd => self.b.ins().iadd(l, r),
                    Bin::Isub => self.b.ins().isub(l, r),
                    Bin::Imul => self.b.ins().imul(l, r),
                    Bin::Fadd => self.b.ins().fadd(l, r),
                    Bin::Fsub => self.b.ins().fsub(l, r),
                    Bin::Fmul => self.b.ins().fmul(l, r),
                    Bin::Fdiv => self.b.ins().fdiv(l, r),
                    Bin::Frem => match t {
                        types::F32 => self.call_stub("rt_fremf", &[t, t], Some(t), &[l, r]).unwrap(),
                        _ => self.call_stub("rt_frem", &[t, t], Some(t), &[l, r]).unwrap(),
                    },
                    Bin::Sdiv | Bin::Srem | Bin::Udiv | Bin::Urem => {
                        let throw = self.throw_block("rt_throw_div", &[]);
                        let cont = self.b.create_block();
                        self.b.ins().brif(r, cont, &[], throw, &[]);
                        self.b.switch_to_block(cont);
                        match bin {
                            Bin::Udiv => self.b.ins().udiv(l, r),
                            Bin::Urem => self.b.ins().urem(l, r),
                            _ => {
                                let minus_one = self.b.ins().icmp_imm_s(IntCC::Equal, r, -1);
                                let one = self.b.ins().iconst(t, 1);
                                let safe = self.b.ins().select(minus_one, one, r);
                                if matches!(bin, Bin::Sdiv) {
                                    let q = self.b.ins().sdiv(l, safe);
                                    let neg = self.b.ins().ineg(l);
                                    self.b.ins().select(minus_one, neg, q)
                                } else {
                                    let q = self.b.ins().srem(l, safe);
                                    let zero = self.b.ins().iconst(t, 0);
                                    self.b.ins().select(minus_one, zero, q)
                                }
                            }
                        }
                    }
                    Bin::Shl => self.b.ins().ishl(l, r),
                    Bin::Lshr => self.b.ins().ushr(l, r),
                    Bin::Ashr => self.b.ins().sshr(l, r),
                    Bin::And => self.b.ins().band(l, r),
                    Bin::Or => self.b.ins().bor(l, r),
                    Bin::Xor => self.b.ins().bxor(l, r),
                }
            }
            Op::Comp { comp, l, r, .. } => {
                let (l, r) = (self.val(*l)?, self.val(*r)?);
                match comp {
                    Comp::Ieq => self.b.ins().icmp(IntCC::Equal, l, r),
                    Comp::Ine => self.b.ins().icmp(IntCC::NotEqual, l, r),
                    Comp::Ugt => self.b.ins().icmp(IntCC::UnsignedGreaterThan, l, r),
                    Comp::Uge => self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, l, r),
                    Comp::Ult => self.b.ins().icmp(IntCC::UnsignedLessThan, l, r),
                    Comp::Ule => self.b.ins().icmp(IntCC::UnsignedLessThanOrEqual, l, r),
                    Comp::Sgt => self.b.ins().icmp(IntCC::SignedGreaterThan, l, r),
                    Comp::Sge => self.b.ins().icmp(IntCC::SignedGreaterThanOrEqual, l, r),
                    Comp::Slt => self.b.ins().icmp(IntCC::SignedLessThan, l, r),
                    Comp::Sle => self.b.ins().icmp(IntCC::SignedLessThanOrEqual, l, r),
                    Comp::Feq => self.b.ins().fcmp(FloatCC::Equal, l, r),
                    Comp::Fne => self.b.ins().fcmp(FloatCC::NotEqual, l, r),
                    Comp::Fgt => self.b.ins().fcmp(FloatCC::GreaterThan, l, r),
                    Comp::Fge => self.b.ins().fcmp(FloatCC::GreaterThanOrEqual, l, r),
                    Comp::Flt => self.b.ins().fcmp(FloatCC::LessThan, l, r),
                    Comp::Fle => self.b.ins().fcmp(FloatCC::LessThanOrEqual, l, r),
                }
            }
            Op::Conv { conv, ty, value } => {
                let v = self.val(*value)?;
                let from = self.b.func.dfg.value_type(v);
                let to = cl_type(pool, *ty)?;
                match conv {
                    Conv::Trunc => {
                        if to.bits() < from.bits() { self.b.ins().ireduce(to, v) } else { v }
                    }
                    Conv::Zext | Conv::ZSizeCast => {
                        if to.bits() > from.bits() {
                            self.b.ins().uextend(to, v)
                        } else if to.bits() < from.bits() {
                            self.b.ins().ireduce(to, v)
                        } else {
                            v
                        }
                    }
                    Conv::Sext | Conv::SSizeCast => {
                        if to.bits() > from.bits() {
                            self.b.ins().sextend(to, v)
                        } else if to.bits() < from.bits() {
                            self.b.ins().ireduce(to, v)
                        } else {
                            v
                        }
                    }
                    Conv::Fptrunc => self.b.ins().fdemote(to, v),
                    Conv::Fpext => self.b.ins().fpromote(to, v),
                    Conv::Fptoui => self.b.ins().fcvt_to_uint_sat(to, v),
                    Conv::Fptosi => self.b.ins().fcvt_to_sint_sat(to, v),
                    Conv::Uitofp => self.b.ins().fcvt_from_uint(to, v),
                    Conv::Sitofp => self.b.ins().fcvt_from_sint(to, v),
                    Conv::Ptrtoint | Conv::Inttoptr | Conv::Bitcast => {
                        if from == to {
                            v
                        } else if from.is_int() && to.is_int() {
                            if to.bits() > from.bits() { self.b.ins().uextend(to, v) } else { self.b.ins().ireduce(to, v) }
                        } else if from.bits() == to.bits() {
                            self.b.ins().bitcast(to, flags, v)
                        } else {
                            return Err(format!("bitcast {from} to {to}"));
                        }
                    }
                }
            }
            Op::Fence(_) => {
                self.b.ins().fence();
                return Ok(None);
            }
            Op::Classalloc { name, .. } => {
                let c = self.b.ins().iconst(types::I64, name.0 as i64);
                self.call_stub("rt_classalloc", &[types::I64], Some(types::I64), &[c]).unwrap()
            }
            Op::Fieldload { ty, obj, name } => {
                let o = self.val(*obj)?;
                self.null_check(o);
                let off = self.fake_offset(*name);
                self.b.ins().load(cl_type(pool, *ty)?, flags, o, off)
            }
            Op::Fieldstore { obj, name, value, .. } => {
                let o = self.val(*obj)?;
                let v = self.val(*value)?;
                self.null_check(o);
                let off = self.fake_offset(*name);
                self.b.ins().store(flags, v, o, off);
                return Ok(None);
            }
            Op::Field { obj, name } => {
                let o = self.val(*obj)?;
                let off = self.fake_offset(*name);
                self.b.ins().iadd_imm_s(o, off as i64)
            }
            Op::Method { obj, sig } => {
                if let Val::Local(l, _) = pool.val(*obj) {
                    if let Some(&m) = self.local_module.get(&l.0) {
                        let member = nir::Global::Member(m, *sig);
                        let target = self.sh.funcs.iter().find(|(g, _)| pool.global(**g) == &member).map(|(_, f)| *f);
                        if let Some(fid) = target {
                            self.local_func.insert(id, fid);
                            let fref = self.module.declare_func_in_func(fid, self.b.func);
                            return Ok(Some(self.b.ins().func_addr(types::I64, fref)));
                        }
                    }
                }
                let o = self.val(*obj)?;
                self.null_check(o);
                let s = self.b.ins().iconst(types::I64, sig.0 as i64);
                self.call_stub("rt_method", &[types::I64, types::I64], Some(types::I64), &[o, s]).unwrap()
            }
            Op::Dynmethod { obj, sig } => {
                let o = self.val(*obj)?;
                let s = self.b.ins().iconst(types::I64, sig.0 as i64);
                self.call_stub("rt_dynmethod", &[types::I64, types::I64], Some(types::I64), &[o, s]).unwrap()
            }
            Op::Module(g) => {
                self.local_module.insert(id, *g);
                let m = self.b.ins().iconst(types::I64, g.0 as i64);
                self.call_stub("rt_module", &[types::I64], Some(types::I64), &[m]).unwrap()
            }
            Op::As { ty, obj } => {
                let o = self.val(*obj)?;
                match pool.ty(*ty) {
                    Type::Ref { .. } | Type::Array { .. } => {
                        let t = self.b.ins().iconst(types::I64, ty.0 as i64);
                        self.call_stub("rt_as", &[types::I64, types::I64], Some(types::I64), &[o, t]).unwrap()
                    }
                    _ => o,
                }
            }
            Op::Is { ty, obj } => {
                let o = self.val(*obj)?;
                let t = self.b.ins().iconst(types::I64, ty.0 as i64);
                self.call_stub("rt_is", &[types::I64, types::I64], Some(types::I8), &[o, t]).unwrap()
            }
            Op::Copy(v) => self.val(*v)?,
            Op::SizeOf(ty) => self.b.ins().iconst(types::I64, size_align(pool, *ty)?.0 as i64),
            Op::AlignmentOf(ty) => self.b.ins().iconst(types::I64, size_align(pool, *ty)?.1 as i64),
            Op::Box { ty, obj } => {
                let v = self.val(*obj)?;
                let bits = self.int_to_i64(v);
                let t = self.b.ins().iconst(types::I64, ty.0 as i64);
                self.call_stub("rt_box", &[types::I64, types::I64], Some(types::I64), &[t, bits]).unwrap()
            }
            Op::Unbox { ty, obj } => {
                let o = self.val(*obj)?;
                let t = self.b.ins().iconst(types::I64, ty.0 as i64);
                let bits = self.call_stub("rt_unbox", &[types::I64, types::I64], Some(types::I64), &[t, o]).unwrap();
                let target = self.unboxed_type(*ty);
                self.from_i64(bits, target)
            }
            Op::Var(ty) => {
                let (size, align) = size_align(pool, *ty)?;
                let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    size.max(1),
                    align.trailing_zeros() as u8,
                ));
                self.var_types.insert(id, *ty);
                self.b.ins().stack_addr(types::I64, slot, 0)
            }
            Op::Varload(slot) => {
                let Val::Local(l, _) = pool.val(*slot) else { return Err("varload".into()) };
                let ty = *self.var_types.get(&l.0).ok_or("var type")?;
                let p = self.val(*slot)?;
                self.b.ins().load(cl_type(pool, ty)?, flags, p, 0)
            }
            Op::Varstore { slot, value } => {
                let p = self.val(*slot)?;
                let v = self.val(*value)?;
                self.b.ins().store(flags, v, p, 0);
                return Ok(None);
            }
            Op::Arrayalloc { ty, init, .. } => {
                let len = match pool.val(*init) {
                    Val::ArrayValue(_, elems) => self.b.ins().iconst(types::I64, elems.len() as i64),
                    _ => {
                        let n = self.val(*init)?;
                        self.int_to_i64(n)
                    }
                };
                let t = self.b.ins().iconst(types::I64, ty.0 as i64);
                self.call_stub("rt_arrayalloc", &[types::I64, types::I64], Some(types::I64), &[t, len]).unwrap()
            }
            Op::Arrayload { ty, arr, idx } => {
                let (a, i) = (self.val(*arr)?, self.val(*idx)?);
                let t = cl_type(pool, *ty)?;
                let addr = self.array_elem_addr(a, i, t.bytes());
                self.b.ins().load(t, flags, addr, 0)
            }
            Op::Arraystore { ty, arr, idx, value } => {
                let (a, i, v) = (self.val(*arr)?, self.val(*idx)?, self.val(*value)?);
                let t = cl_type(pool, *ty)?;
                let addr = self.array_elem_addr(a, i, t.bytes());
                self.b.ins().store(flags, v, addr, 0);
                return Ok(None);
            }
            Op::Arraylength(arr) => {
                let a = self.val(*arr)?;
                self.null_check(a);
                self.b.ins().load(types::I32, flags, a, 8)
            }
        }))
    }
}
