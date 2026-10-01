//! The library symbols the lowering refers to by name (dotty's `Definitions`, nscplugin's
//! `NirDefinitions`).

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub struct Defn {
    pub object: SymbolId,
    pub any: SymbolId,
    pub any_ref: SymbolId,
    pub class: SymbolId,
    pub module_serialization_proxy: SymbolId,
    pub module_serialization_proxy_ctor: SymbolId,
}

impl Defn {
    pub fn new(u: &mut Universe) -> Result<Defn, String> {
        let java_lang = package(u, "java.lang")?;
        let runtime = package(u, "scala.runtime")?;
        let proxy = member(u, runtime, "ModuleSerializationProxy", Kind::Class)?;
        let b = u.builtins();
        Ok(Defn {
            object: b.object,
            any: b.any,
            any_ref: b.any_ref,
            class: member(u, java_lang, "Class", Kind::Class)?,
            module_serialization_proxy: proxy,
            module_serialization_proxy_ctor: member(u, proxy, "<init>", Kind::Def)?,
        })
    }
}

fn package(u: &Universe, name: &str) -> Result<SymbolId, String> {
    u.package(name).ok_or_else(|| format!("package {name} missing from the library"))
}

fn member(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind) -> Result<SymbolId, String> {
    let found = u.decl(owner, name).map_err(|e| e.to_string())?;
    found
        .into_iter()
        .find(|&s| u.symbol(s).kind == kind)
        .ok_or_else(|| format!("{}.{name} missing from the library", u.full_name(owner)))
}

/// `class[args]`.
pub fn applied(u: &mut Universe, class: SymbolId, args: Vec<TypeId>) -> TypeId {
    let tycon = type_ref(u, class);
    u.intern_type(Type::Applied(tycon, args))
}

/// The type of a reference to `s` from where its owner's members are visible.
pub fn type_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let prefix = match u.symbol(s).owner {
        Some(o) if u.symbol(o).kind == Kind::Package => Some(u.intern_type(Type::Package(o))),
        Some(o) if u.symbol(o).kind == Kind::Class => {
            let class = type_ref(u, o);
            Some(u.intern_type(Type::This(class)))
        }
        _ => None,
    };
    u.intern_type(Type::Sym(prefix, s))
}
