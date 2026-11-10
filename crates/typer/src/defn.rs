// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! The well-known symbols of `tasty::defn`, and the types of references to symbols.

use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub use tasty::defn::*;

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
    u.this_of(t).filter(|&c| matches!(u.ty(c), Type::Package(_))).unwrap_or(t)
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
    let prefix = match prefix.map(|p| u.ty(package_ref(u, p)).clone()) {
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
