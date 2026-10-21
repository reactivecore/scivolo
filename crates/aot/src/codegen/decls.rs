//! The object's symbols: functions, class metadata, module load functions and runtime imports,
//! declared on first use.

use super::RttiLayout;
use crate::R;
use crate::classes::ClassId;
use crate::reach::Reach;
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{DefnKind, GlobalId, Pool, Type, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

/// The C name of extern member `g`; an `llvm.` intrinsic's is its C implementation's.
pub fn extern_name(pool: &Pool, g: GlobalId) -> Option<String> {
    let name = nir::demangle::extern_name(pool.member_sig(g)?)?;
    Some(match name.strip_prefix("llvm.") {
        Some(intrinsic) => format!("scivolo_aot_llvm_{}", intrinsic.replace('.', "_")),
        None => name,
    })
}

pub struct Decls {
    pub call_conv: CallConv,
    pub ptr: types::Type,
    pub rtti: RttiLayout,
    pub module_slots: DataId,
    /// `scivolo_aot_exception`, the thrown object waiting for a handler.
    pub exception: DataId,
    /// `scivolo_aot_stack_limit`, below which a function throws StackOverflowError.
    pub stack_limit: DataId,
    /// Whether functions check the stack and appear in stack traces (not with `--entry`).
    pub stack_checks: bool,
    /// Functions written without NIR so far.
    pub generated: FxHashSet<String>,
    pub frames: super::frames::Frames,
    pub all_classes: DataId,
    funcs: FxHashMap<GlobalId, FuncId>,
    rttis: FxHashMap<ClassId, DataId>,
    loads: FxHashMap<ClassId, FuncId>,
    imports: FxHashMap<&'static str, FuncId>,
    pub strings: FxHashMap<Vec<u16>, DataId>,
    /// Constants defined so far, which number their names.
    pub anon: usize,
    /// The C source of each variadic call shim.
    pub shims: Vec<String>,
    pub globals: FxHashMap<GlobalId, DataId>,
    /// Global variables and their sizes, which the collector scans.
    pub roots: Vec<(DataId, usize)>,
    pub linktime: super::linktime::Linktime,
}

impl Decls {
    pub fn new(module: &mut ObjectModule, rtti: RttiLayout) -> R<Self> {
        let mut declare = |name: &str, linkage, writable| {
            module.declare_data(name, linkage, writable, false).map_err(|e| e.to_string())
        };
        let module_slots = declare("scivolo_aot_modules", Linkage::Export, true)?;
        let exception = declare("scivolo_aot_exception", Linkage::Import, true)?;
        let stack_limit = declare("scivolo_aot_stack_limit", Linkage::Import, true)?;
        let all_classes = declare("scivolo_aot_classes", Linkage::Local, false)?;
        Ok(Decls {
            exception,
            stack_limit,
            stack_checks: true,
            generated: FxHashSet::default(),
            frames: Default::default(),
            all_classes,
            call_conv: module.isa().default_call_conv(),
            ptr: module.target_config().pointer_type(),
            rtti,
            module_slots,
            funcs: FxHashMap::default(),
            rttis: FxHashMap::default(),
            loads: FxHashMap::default(),
            imports: FxHashMap::default(),
            strings: FxHashMap::default(),
            anon: 0,
            shims: Vec::new(),
            globals: FxHashMap::default(),
            roots: Vec::new(),
            linktime: Default::default(),
        })
    }

    pub fn has_func(&self, g: GlobalId) -> bool {
        self.funcs.contains_key(&g)
    }

    /// Function `g`, reached for compilation, or the C function an extern `g` names.
    pub fn func(&mut self, module: &mut ObjectModule, reach: &mut Reach, g: GlobalId) -> R<FuncId> {
        if let Some(&id) = self.funcs.get(&g) {
            return Ok(id);
        }
        let defn = reach.world.defn(g).map_err(|e| e.to_string())?;
        let replaced = super::replaced::c_replacement(&reach.world.pool, g).map(str::to_owned);
        let c_name = match &defn.kind {
            DefnKind::Declare { .. } => extern_name(&reach.world.pool, g),
            _ => replaced,
        };
        if let (DefnKind::Declare { ty } | DefnKind::Define { ty, .. }, Some(name)) = (&defn.kind, c_name) {
            let sig = self.signature(&reach.world.pool, *ty)?;
            let id = module.declare_function(&name, Linkage::Import, &sig).map_err(|e| e.to_string())?;
            self.funcs.insert(g, id);
            return Ok(id);
        }
        reach.func(g)?;
        let defn = reach.world.defn(g).map_err(|e| e.to_string())?;
        let DefnKind::Define { ty, .. } = defn.kind else { unreachable!("reached a non-function") };
        let sig = self.signature(&reach.world.pool, ty)?;
        let name = format!("_S{}", reach.world.pool.mangle(g));
        let id = module.declare_function(&name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        self.funcs.insert(g, id);
        Ok(id)
    }

    /// The metadata of `cls`, defined with the tables.
    pub fn rtti(&mut self, module: &mut ObjectModule, reach: &Reach, cls: ClassId) -> R<DataId> {
        if let Some(&id) = self.rttis.get(&cls) {
            return Ok(id);
        }
        let name = format!("_S{}", reach.world.pool.mangle(reach.classes.get(cls).name));
        let id = module.declare_data(&name, Linkage::Local, false, false).map_err(|e| e.to_string())?;
        self.rttis.insert(cls, id);
        Ok(id)
    }

    /// The load function of module `cls`, which returns its instance; defined with the tables.
    pub fn module_load(&mut self, module: &mut ObjectModule, reach: &mut Reach, cls: ClassId) -> R<FuncId> {
        if let Some(&id) = self.loads.get(&cls) {
            return Ok(id);
        }
        reach.module(cls)?;
        if let Some(ctor) = reach.constructor(cls) {
            self.func(module, reach, ctor)?;
        }
        let mut sig = Signature::new(self.call_conv);
        sig.returns.push(AbiParam::new(self.ptr));
        let name = format!("_S{}.load", reach.world.pool.mangle(reach.classes.get(cls).name));
        let id = module.declare_function(&name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        self.loads.insert(cls, id);
        Ok(id)
    }

    /// The C function `name`, taking `params` and returning `ret`.
    pub fn import(
        &mut self,
        module: &mut ObjectModule,
        name: &'static str,
        params: &[types::Type],
        ret: Option<types::Type>,
    ) -> R<FuncId> {
        if let Some(&id) = self.imports.get(name) {
            return Ok(id);
        }
        let mut sig = Signature::new(self.call_conv);
        sig.params.extend(params.iter().map(|&t| AbiParam::new(t)));
        sig.returns.extend(ret.map(AbiParam::new));
        let id = module.declare_function(name, Linkage::Import, &sig).map_err(|e| e.to_string())?;
        self.imports.insert(name, id);
        Ok(id)
    }

    pub fn signature(&self, pool: &Pool, ty: TypeId) -> R<Signature> {
        let Type::Function(params, ret) = pool.ty(ty) else { return Err("not a function type".into()) };
        let mut sig = Signature::new(self.call_conv);
        for &p in params {
            sig.params.push(self.abi_param(pool, p)?);
        }
        if !matches!(pool.ty(*ret), Type::Unit | Type::Nothing) {
            sig.returns.push(self.abi_param(pool, *ret)?);
        }
        Ok(sig)
    }

    /// A parameter or result of NIR type `t`: bytes and shorts sign-extended, booleans and
    /// chars zero-extended, as C expects them.
    pub fn abi_param(&self, pool: &Pool, t: TypeId) -> R<AbiParam> {
        let param = AbiParam::new(self.value_type(pool, t)?);
        Ok(match pool.ty(t) {
            Type::Byte | Type::Short => param.sext(),
            Type::Bool | Type::Char => param.uext(),
            _ => param,
        })
    }

    /// The Cranelift type of NIR values of type `t`.
    pub fn value_type(&self, pool: &Pool, t: TypeId) -> R<types::Type> {
        Ok(match pool.ty(t) {
            Type::Bool | Type::Byte | Type::Unit | Type::Nothing => types::I8,
            Type::Char | Type::Short => types::I16,
            Type::Int => types::I32,
            Type::Long => types::I64,
            Type::Size | Type::Ptr | Type::Null | Type::Virtual => self.ptr,
            Type::Array { .. } | Type::Ref { .. } | Type::Var(_) | Type::Function(..) => self.ptr,
            Type::Float => types::F32,
            Type::Double => types::F64,
            t => return Err(format!("value type {t:?}")),
        })
    }
}
