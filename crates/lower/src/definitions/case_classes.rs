use super::equality::method;
use super::{java_static, member, package, type_ref};
use tasty::{Kind, SymbolId, Type, Universe};

/// What the synthetic members of case classes and their companions refer to.
#[derive(Clone, Copy)]
pub struct CaseClasses {
    pub hash_code: SymbolId,
    pub to_string: SymbolId,
    pub can_equal: SymbolId,
    pub product_arity: SymbolId,
    pub product_prefix: SymbolId,
    pub product_element: SymbolId,
    pub product_element_name: SymbolId,
    pub product: SymbolId,
    /// `Object.eq`, `Boolean.||`.
    pub eq: SymbolId,
    pub or: SymbolId,
    /// `ScalaRunTime._toString`, its object.
    pub runtime_to_string: SymbolId,
    pub scala_run_time: SymbolId,
    /// `Statics.mix`, `finalizeHash`, `longHash`, `doubleHash`, `floatHash`: Java statics.
    pub mix: SymbolId,
    pub finalize_hash: SymbolId,
    pub long_hash: SymbolId,
    pub double_hash: SymbolId,
    pub float_hash: SymbolId,
    /// `MurmurHash3.productHash(x, seed, ignorePrefix)`, its object.
    pub product_hash: SymbolId,
    pub murmur_hash3: SymbolId,
    /// `IndexOutOfBoundsException` and its constructor taking an `Int`.
    pub ioobe: SymbolId,
    pub ioobe_ctor: SymbolId,
    /// `deriving.Mirror.Product`, `deriving.Mirror.Singleton`.
    pub mirror_product: SymbolId,
    pub mirror_singleton: SymbolId,
}

impl CaseClasses {
    pub fn new(
        u: &mut Universe,
        any: SymbolId,
        object: SymbolId,
        statics: SymbolId,
    ) -> Result<CaseClasses, String> {
        let scala = package(u, "scala")?;
        let product = member(u, scala, "Product", Kind::Class)?;
        let equals = member(u, scala, "Equals", Kind::Class)?;
        let boolean = member(u, scala, "Boolean", Kind::Class)?;
        let run_time = package(u, "scala.runtime")?;
        let scala_run_time = member(u, run_time, "ScalaRunTime", Kind::Val)?;
        let run_time_class = module_class(u, scala_run_time)?;
        let hashing = package(u, "scala.util.hashing")?;
        let murmur_hash3 = member(u, hashing, "MurmurHash3", Kind::Val)?;
        let murmur_class = module_class(u, murmur_hash3)?;
        let java_lang = package(u, "java.lang")?;
        let ioobe = member(u, java_lang, "IndexOutOfBoundsException", Kind::Class)?;
        let deriving = package(u, "scala.deriving")?;
        let mirror = member(u, deriving, "Mirror", Kind::Val)?;
        let mirror_class = module_class(u, mirror)?;
        let class_type = |u: &mut Universe, name: &str| -> Result<tasty::TypeId, String> {
            let class = member(u, scala, name, Kind::Class)?;
            Ok(type_ref(u, class))
        };
        let int = class_type(u, "Int")?;
        let long = class_type(u, "Long")?;
        let double = class_type(u, "Double")?;
        let float = class_type(u, "Float")?;
        let statics_method = |u: &mut Universe, name: &str, params: &[tasty::TypeId]| {
            let info = method(u, params, int);
            java_static(u, statics, name, Kind::Def, info)
        };
        Ok(CaseClasses {
            hash_code: member(u, any, "hashCode", Kind::Def)?,
            to_string: member(u, any, "toString", Kind::Def)?,
            can_equal: member(u, equals, "canEqual", Kind::Def)?,
            product_arity: member(u, product, "productArity", Kind::Def)?,
            product_prefix: member(u, product, "productPrefix", Kind::Def)?,
            product_element: member(u, product, "productElement", Kind::Def)?,
            product_element_name: member(u, product, "productElementName", Kind::Def)?,
            product,
            eq: member(u, object, "eq", Kind::Def)?,
            or: member(u, boolean, "||", Kind::Def)?,
            runtime_to_string: member(u, run_time_class, "_toString", Kind::Def)?,
            scala_run_time,
            mix: statics_method(u, "mix", &[int, int]),
            finalize_hash: statics_method(u, "finalizeHash", &[int, int]),
            long_hash: statics_method(u, "longHash", &[long]),
            double_hash: statics_method(u, "doubleHash", &[double]),
            float_hash: statics_method(u, "floatHash", &[float]),
            product_hash: with_params(u, murmur_class, "productHash", 3)?,
            murmur_hash3,
            ioobe,
            ioobe_ctor: with_params(u, ioobe, "<init>", 1).and_then(|c| int_param(u, ioobe, c))?,
            mirror_product: member(u, mirror_class, "Product", Kind::Class)?,
            mirror_singleton: member(u, mirror_class, "Singleton", Kind::Class)?,
        })
    }
}

fn module_class(u: &mut Universe, module: SymbolId) -> Result<SymbolId, String> {
    u.module_class(module).ok_or_else(|| format!("{}'s class missing", u.full_name(module)))
}

/// The method `name` of `owner` or a base class with `n` parameters.
fn with_params(u: &mut Universe, owner: SymbolId, name: &str, n: usize) -> Result<SymbolId, String> {
    let mut found = Vec::new();
    for base in u.linearization(owner) {
        found.extend(u.decl(base, name).unwrap_or_default());
    }
    found
        .into_iter()
        .find(|&m| {
            let info = u.info(m);
            matches!(u.types().get(info), Type::Method { params, .. } if params.len() == n)
        })
        .ok_or_else(|| format!("{}.{name} with {n} parameters missing", u.full_name(owner)))
}

/// The constructor of `class` taking an `Int`, `first` if no other one does.
fn int_param(u: &mut Universe, class: SymbolId, first: SymbolId) -> Result<SymbolId, String> {
    let ctors = u.decl(class, "<init>").map_err(|e| e.to_string())?;
    let int = |u: &mut Universe, m: SymbolId| {
        let info = u.info(m);
        let Type::Method { params, .. } = u.types().get(info).clone() else { return false };
        params.len() == 1 && u.class_symbol(params[0].1).is_some_and(|c| u.full_name(c) == "scala.Int")
    };
    Ok(ctors.into_iter().find(|&c| int(u, c)).unwrap_or(first))
}
