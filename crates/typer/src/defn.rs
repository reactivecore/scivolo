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

/// The declaration `name` of `kind` already entered in `owner`, loading nothing.
pub fn entered(u: &Universe, owner: SymbolId, name: &str, kind: Kind) -> Option<SymbolId> {
    u.symbol(owner).decls.iter().copied().find(|&d| u.symbol(d).name == name && u.symbol(d).kind == kind)
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

/// The type of a reference to `sym` through `prefix` (dotty's `TermRef`/`TypeRef`), a member of
/// a package object through the object (`makePackageObjPrefixExplicit`).
pub fn ref_type(u: &mut Universe, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
    if u.symbol(sym).kind == Kind::Package {
        return u.intern_type(Type::Package(sym));
    }
    let in_package_object =
        |u: &mut Universe, c: TypeId| u.class_symbol(c).is_some_and(|c| is_named_package_object(u, c));
    let prefix = match prefix.map(|p| u.types().get(p).clone()) {
        Some(Type::Package(_)) => package_object_prefix(u, sym).or(prefix),
        Some(Type::This(c)) if in_package_object(u, c) => package_object_prefix(u, sym).or(prefix),
        _ => prefix,
    };
    u.intern_type(Type::Sym(prefix, sym))
}

/// Whether `c` is declared in a package under a package object's class name: `package$`, or a
/// file's `<name>$package$`.
pub fn is_named_package_object(u: &Universe, c: SymbolId) -> bool {
    let s = u.symbol(c);
    s.owner.is_some_and(|o| u.symbol(o).kind == Kind::Package) && tasty::is_package_object_name(&s.name)
}

/// The reference to the package object that declares `sym`, if one does.
fn package_object_prefix(u: &mut Universe, sym: SymbolId) -> Option<TypeId> {
    let obj_class = u.symbol(sym).owner?;
    let pkg = u.symbol(obj_class).owner?;
    if u.symbol(obj_class).kind != Kind::Class || u.symbol(pkg).kind != Kind::Package {
        return None;
    }
    let module = u.source_module(obj_class)?;
    Some(type_ref(u, module))
}

/// `class[args]`.
pub fn applied(u: &mut Universe, class: SymbolId, args: Vec<TypeId>) -> TypeId {
    let tycon = type_ref(u, class);
    u.intern_type(Type::Applied(tycon, args))
}
