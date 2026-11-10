// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! The library symbols the typer refers to by name (dotty's `Definitions`).

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub const ARRAY: &str = "scala.Array";
pub const INT: &str = "scala.Int";
pub const SOME: &str = "scala.Some";
/// The class of tuples, and the prefix of the names of `TupleN`.
pub const TUPLE: &str = "scala.Tuple";
pub const IMMUTABLE_SEQ: &str = "scala.collection.immutable.Seq";
/// The class of case classes and tuples, whose selectors `_1`, ..., `_n` patterns match.
pub const PRODUCT: &str = "scala.Product";
pub const EQUALS: &str = "scala.Equals";
/// The type class of equality evidence.
pub const CAN_EQUAL: &str = "scala.CanEqual";
pub const CLASS_TAG: &str = "scala.reflect.ClassTag";
pub const TYPE_TEST: &str = "scala.reflect.TypeTest";
pub const MANIFEST: &str = "scala.reflect.Manifest";
pub const OPT_MANIFEST: &str = "scala.reflect.OptManifest";
pub const VALUE_OF: &str = "scala.ValueOf";
pub const TUPLED_FUNCTION: &str = "scala.util.TupledFunction";
pub const MIRROR: &str = "scala.deriving.Mirror";
pub const MIRROR_PRODUCT: &str = "scala.deriving.Mirror$.Product";
pub const MIRROR_SUM: &str = "scala.deriving.Mirror$.Sum";
/// The annotation of the types of `inline` parameters.
pub const INLINE_PARAM: &str = "scala.annotation.internal.InlineParam";

/// The symbols every unit needs, and those resolved on first use.
pub struct Defn {
    pub scala: SymbolId,
    pub java_lang: SymbolId,
    /// The `Predef` object, its value.
    pub predef: SymbolId,
    pub unit: SymbolId,
    pub object: SymbolId,
    pub string: SymbolId,
    throwable: Option<SymbolId>,
    array: Option<SymbolId>,
    main_annot: Option<SymbolId>,
    command_line_parser: Option<SymbolId>,
}

impl Defn {
    pub fn new(u: &mut Universe) -> Result<Defn, String> {
        let scala = package(u, "scala")?;
        let java_lang = package(u, "java.lang")?;
        Ok(Defn {
            scala,
            java_lang,
            predef: member(u, scala, "Predef", Kind::Val)?,
            unit: member(u, scala, "Unit", Kind::Class)?,
            object: member(u, java_lang, "Object", Kind::Class)?,
            string: member(u, java_lang, "String", Kind::Class)?,
            throwable: None,
            array: None,
            main_annot: None,
            command_line_parser: None,
        })
    }

    pub fn throwable(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.throwable, u, self.java_lang, "Throwable", Kind::Class)
    }

    pub fn array(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.array, u, self.scala, "Array", Kind::Class)
    }

    pub fn main_annot(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        resolved(&mut self.main_annot, u, self.scala, "main", Kind::Class)
    }

    /// The `scala.util.CommandLineParser` object, its value.
    pub fn command_line_parser(&mut self, u: &mut Universe) -> Result<SymbolId, String> {
        if let Some(s) = self.command_line_parser {
            return Ok(s);
        }
        let util = package(u, "scala.util")?;
        resolved(&mut self.command_line_parser, u, util, "CommandLineParser", Kind::Val)
    }
}

/// `slot`, else the member `name` of `kind` of `owner`, kept in `slot`.
fn resolved(
    slot: &mut Option<SymbolId>,
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    kind: Kind,
) -> Result<SymbolId, String> {
    if let Some(s) = *slot {
        return Ok(s);
    }
    let s = member(u, owner, name, kind)?;
    *slot = Some(s);
    Ok(s)
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

/// Prefix `t`, the package's reference for the `this` of a package.
pub fn package_ref(u: &Universe, t: TypeId) -> TypeId {
    match *u.types().get(t) {
        Type::This(c) if matches!(u.types().get(c), Type::Package(_)) => c,
        _ => t,
    }
}

/// The type of a reference to `s` from where its owner's members are visible (dotty's
/// `typeRef`/`termRef`); a local symbol, owned by a term, and a member of a package have no
/// prefix.
pub fn type_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let class = u.symbol(s).kind == Kind::Class;
    let class_owner = |o: SymbolId| match u.symbol(o).kind {
        Kind::Class => true,
        Kind::Type | Kind::TypeParam => !class,
        _ => false,
    };
    let prefix = u.symbol(s).owner.filter(|&o| class_owner(o)).map(|o| owner_prefix(u, o));
    u.intern_type(Type::Sym(prefix, s))
}

/// The type of a reference to `sym` through `prefix` (dotty's `TermRef`/`TypeRef`), a member of
/// a package object through the object (`makePackageObjPrefixExplicit`) but an opaque type of
/// the unit through its `this`.
pub fn ref_type(u: &mut Universe, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
    if u.symbol(sym).kind == Kind::Package {
        return u.intern_type(Type::Package(sym));
    }
    let s = u.symbol(sym);
    let opaque = s.kind == Kind::Type && s.flags.has(tasty::tags::OPAQUE) && s.tree.is_none();
    let in_package_object = |u: &mut Universe, c: TypeId| {
        !opaque && u.class_symbol(c).is_some_and(|c| is_named_package_object(u, c))
    };
    let prefix = match prefix.map(|p| u.types().get(package_ref(u, p)).clone()) {
        Some(Type::Package(p)) => package_object_prefix(u, Some(p), sym).or(prefix),
        Some(Type::This(c)) if in_package_object(u, c) => package_object_prefix(u, None, sym).or(prefix),
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

/// The reference to the package object that declares `sym`, if one does, else to the object
/// `package` of package `pkg` if it inherits `sym` (dotty's `packageObjFor`).
fn package_object_prefix(u: &mut Universe, pkg: Option<SymbolId>, sym: SymbolId) -> Option<TypeId> {
    let owner = u.symbol(sym).owner.filter(|&o| u.symbol(o).kind == Kind::Class)?;
    let inheriting = pkg.and_then(|pkg| inheriting_package_object(u, pkg, owner));
    let in_package = u.symbol(owner).owner.is_some_and(|o| u.symbol(o).kind == Kind::Package);
    let module = u.source_module(inheriting.or(in_package.then_some(owner))?)?;
    Some(type_ref(u, module))
}

/// The class of the object `package` of package `pkg` if it inherits the members of class
/// `owner`, another class.
pub fn inheriting_package_object(u: &mut Universe, pkg: SymbolId, owner: SymbolId) -> Option<SymbolId> {
    let objects = u.package_objects(pkg).ok()?;
    objects
        .into_iter()
        .find(|&o| o != owner && u.symbol(o).name == "package$" && u.linearization(o).contains(&owner))
}

/// `class[args]`.
pub fn applied(u: &mut Universe, class: SymbolId, args: Vec<TypeId>) -> TypeId {
    let tycon = type_ref(u, class);
    u.intern_type(Type::Applied(tycon, args))
}
