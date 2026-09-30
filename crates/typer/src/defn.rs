//! The library symbols the typer refers to by name (dotty's `Definitions`).

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub struct Defn {
    pub scala: SymbolId,
    pub java_lang: SymbolId,
    /// The `Predef` object, its value.
    pub predef: SymbolId,
    pub unit: SymbolId,
    pub throwable: SymbolId,
    pub object: SymbolId,
    pub string: SymbolId,
    pub array: SymbolId,
    pub main_annot: SymbolId,
    /// The `scala.util.CommandLineParser` object, its value.
    pub command_line_parser: SymbolId,
}

impl Defn {
    pub fn new(u: &mut Universe) -> Result<Defn, String> {
        let scala = package(u, "scala")?;
        let java_lang = package(u, "java.lang")?;
        let util = package(u, "scala.util")?;
        Ok(Defn {
            scala,
            java_lang,
            predef: member(u, scala, "Predef", Kind::Val)?,
            unit: member(u, scala, "Unit", Kind::Class)?,
            throwable: member(u, java_lang, "Throwable", Kind::Class)?,
            object: member(u, java_lang, "Object", Kind::Class)?,
            string: member(u, java_lang, "String", Kind::Class)?,
            array: member(u, scala, "Array", Kind::Class)?,
            main_annot: member(u, scala, "main", Kind::Class)?,
            command_line_parser: member(u, util, "CommandLineParser", Kind::Val)?,
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

/// The prefix of references to members of `owner`: a package, or the `this` of a class.
pub fn owner_prefix(u: &mut Universe, owner: SymbolId) -> TypeId {
    match u.symbol(owner).kind {
        Kind::Package => u.intern_type(Type::Package(owner)),
        _ => {
            let class = type_ref(u, owner);
            u.intern_type(Type::This(class))
        }
    }
}

/// The type of a reference to `s` from where its owner's members are visible; a local symbol,
/// owned by a term, has no prefix.
pub fn type_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let prefix = u.symbol(s).owner.filter(|&o| !u.symbol(o).kind.is_term()).map(|o| owner_prefix(u, o));
    u.intern_type(Type::Sym(prefix, s))
}

/// `class[args]`.
pub fn applied(u: &mut Universe, class: SymbolId, args: Vec<TypeId>) -> TypeId {
    let tycon = type_ref(u, class);
    u.intern_type(Type::Applied(tycon, args))
}
