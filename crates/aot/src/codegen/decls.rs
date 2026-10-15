//! The object's symbols: functions, class metadata, module load functions and runtime imports,
//! declared on first use.

use crate::R;
use crate::classes::{ClassId, Classes};
use crate::reach::Reach;
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{DefnKind, GlobalId, Pool, Type, TypeId};
use rustc_hash::FxHashMap;

/// Offsets in a class's metadata: a `java.lang.Class` instance, then its itable and vtable.
pub struct RttiLayout {
    pub id: i32,
    pub range: i32,
    pub size: i32,
    pub interfaces: i32,
    pub interfaces_count: i32,
    pub ref_offsets: i32,
    pub super_class: i32,
    /// Scala Native's itable size; negative makes `Class.is` search the interfaces.
    pub itables_count: i32,
    pub itable: i32,
    pub itable_len: i32,
    pub vtable: i32,
}

pub const CLASS: &str = "java.lang.Class";

/// The C name of extern member `g`: its signature is `C`, the name's length, the name.
fn extern_name(pool: &Pool, g: GlobalId) -> Option<String> {
    let rest = pool.member_sig(g)?.strip_prefix('C')?;
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    Some(rest[digits..].trim_start_matches('-').to_owned())
}

impl RttiLayout {
    pub fn new(reach: &mut Reach) -> R<Self> {
        let meta = reach.link_top(CLASS)?;
        let classes: &Classes = &reach.classes;
        let mut field = |sig: &str| {
            let g = reach.world.member(classes.get(meta).name, sig);
            let f = classes.get(meta).fields.get(&g).ok_or_else(|| format!("no field {CLASS}::{sig}"))?;
            Ok::<_, String>(f.offset as i32)
        };
        for (sig, offset) in vm::GC_RTTI_FIELDS {
            if field(sig)? != offset as i32 {
                return Err(format!("{CLASS}::{sig} is not where Immix reads it"));
            }
        }
        let header = classes.get(meta).size.next_multiple_of(8) as i32;
        Ok(RttiLayout {
            id: field("F2idO")?,
            range: field("F12idRangeUntilO")?,
            size: field("F4sizeO")?,
            interfaces: field("F10interfacesO")?,
            interfaces_count: field("F15interfacesCountO")?,
            ref_offsets: field("F15refFieldOffsetsO")?,
            super_class: field("F10superClassO")?,
            itables_count: field("F12itablesCountO")?,
            itable: header,
            itable_len: header + 8,
            vtable: header + 16,
        })
    }
}

pub struct Decls {
    pub call_conv: CallConv,
    pub ptr: types::Type,
    pub rtti: RttiLayout,
    pub module_slots: DataId,
    /// `scalai_aot_exception`, the thrown object waiting for a handler.
    pub exception: DataId,
    funcs: FxHashMap<GlobalId, FuncId>,
    rttis: FxHashMap<ClassId, DataId>,
    loads: FxHashMap<ClassId, FuncId>,
    imports: FxHashMap<&'static str, FuncId>,
    pub strings: FxHashMap<Vec<u16>, DataId>,
}

impl Decls {
    pub fn new(module: &mut ObjectModule, rtti: RttiLayout) -> R<Self> {
        let module_slots = module
            .declare_data("scalai_aot_modules", Linkage::Export, true, false)
            .map_err(|e| e.to_string())?;
        let exception = module
            .declare_data("scalai_aot_exception", Linkage::Import, true, false)
            .map_err(|e| e.to_string())?;
        Ok(Decls {
            exception,
            call_conv: module.isa().default_call_conv(),
            ptr: module.target_config().pointer_type(),
            rtti,
            module_slots,
            funcs: FxHashMap::default(),
            rttis: FxHashMap::default(),
            loads: FxHashMap::default(),
            imports: FxHashMap::default(),
            strings: FxHashMap::default(),
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
        if let (DefnKind::Declare { ty }, Some(name)) = (&defn.kind, extern_name(&reach.world.pool, g)) {
            // Temporary until #113 aot 5 (externs): variadic externs and VM-only natives are refused.
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
            sig.params.push(AbiParam::new(self.value_type(pool, p)?));
        }
        if !matches!(pool.ty(*ret), Type::Unit | Type::Nothing) {
            sig.returns.push(AbiParam::new(self.value_type(pool, *ret)?));
        }
        Ok(sig)
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
