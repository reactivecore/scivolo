//! The library symbols the lowering refers to by name (dotty's `Definitions`, nscplugin's
//! `NirDefinitions`).

use tasty::tags::STATIC;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId, Universe};

/// A primitive value class: its erased letter, its object and that object's `box` method.
pub struct Primitive {
    pub letter: char,
    pub module: SymbolId,
    pub box_method: SymbolId,
}

pub struct Definitions {
    pub object: SymbolId,
    pub any: SymbolId,
    pub any_ref: SymbolId,
    pub class: SymbolId,
    pub module_serialization_proxy: SymbolId,
    pub module_serialization_proxy_ctor: SymbolId,
    pub primitives: Vec<Primitive>,
    /// `scala.runtime.BoxedUnit.UNIT`, a Java static field.
    pub boxed_unit_unit: SymbolId,
}

/// The primitive value classes of package `scala` and their erased letters.
const PRIMITIVES: [(&str, char); 8] = [
    ("Boolean", 'z'),
    ("Char", 'c'),
    ("Byte", 'b'),
    ("Short", 's'),
    ("Int", 'i'),
    ("Long", 'j'),
    ("Float", 'f'),
    ("Double", 'd'),
];

impl Definitions {
    pub fn new(u: &mut Universe) -> Result<Definitions, String> {
        let java_lang = package(u, "java.lang")?;
        let scala = package(u, "scala")?;
        let runtime = package(u, "scala.runtime")?;
        let proxy = member(u, runtime, "ModuleSerializationProxy", Kind::Class)?;
        let mut primitives = Vec::new();
        for (name, letter) in PRIMITIVES {
            let module = member(u, scala, name, Kind::Val)?;
            let class = u.module_class(module).ok_or_else(|| format!("scala.{name}'s object missing"))?;
            let box_method = member(u, class, "box", Kind::Def)?;
            primitives.push(Primitive { letter, module, box_method });
        }
        let b = u.builtins();
        Ok(Definitions {
            object: b.object,
            any: b.any,
            any_ref: b.any_ref,
            class: member(u, java_lang, "Class", Kind::Class)?,
            module_serialization_proxy: proxy,
            module_serialization_proxy_ctor: member(u, proxy, "<init>", Kind::Def)?,
            primitives,
            boxed_unit_unit: boxed_unit_unit(u, runtime)?,
        })
    }

    pub fn primitive(&self, letter: char) -> Option<&Primitive> {
        self.primitives.iter().find(|p| p.letter == letter)
    }

    pub fn is_box(&self, m: SymbolId) -> bool {
        self.primitives.iter().any(|p| p.box_method == m)
    }
}

/// `BoxedUnit.UNIT`, defined with its class if the library has no TASTy for them.
fn boxed_unit_unit(u: &mut Universe, runtime: SymbolId) -> Result<SymbolId, String> {
    let class = match member(u, runtime, "BoxedUnit", Kind::Class) {
        Ok(c) => c,
        Err(_) => u.define(symbol("BoxedUnit", Kind::Class, runtime, Flags::default())),
    };
    if let Ok(unit) = member(u, class, "UNIT", Kind::Val) {
        return Ok(unit);
    }
    let unit = u.define(symbol("UNIT", Kind::Val, class, Flags::default().with(STATIC)));
    let info = type_ref(u, class);
    u.set_info(unit, info);
    Ok(unit)
}

fn symbol(name: &str, kind: Kind, owner: SymbolId, flags: Flags) -> Symbol {
    Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target: None,
        is_extern: false,
        decls: Vec::new(),
        tree: None,
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
