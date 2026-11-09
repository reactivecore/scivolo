// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//   compiler/src/dotty/tools/dotc/transform/ProtectedAccessors.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! Which references to protected members need accessors, for the TASTy pickler and the lowering.

use tasty::tags::{PROTECTED, PROTECTEDqualified, STATIC};
use tasty::{Kind, SymbolId, Universe};

/// Whether a reference to `sym` from class `class` needs an accessor unless `class` derives from
/// the owner of `sym` (dotty's `needsAccessorIfNotInSubclass`).
pub fn needs_accessor_if_not_in_subclass(u: &mut Universe, sym: SymbolId, class: SymbolId) -> bool {
    let s = u.symbol(sym);
    let (kind, flags, owner) = (s.kind, s.flags, s.owner);
    matches!(kind, Kind::Def | Kind::Val)
        && (flags.has(PROTECTED) || flags.has(PROTECTEDqualified))
        && !u.is_constructor(sym)
        && !u.is_annotated(sym, "scala.annotation.publicInBinary")
        && owner.is_some_and(|o| !u.is_trait(o))
        && !is_java_static(u, sym)
        && !within(u, class, package_of(u, sym))
}

/// Whether `sym` selected on `this` from class `class` needs a super accessor in the trait
/// hosting its accessor (`SuperAccessors.transformSelect`); `Err` for a member of a JDK class but
/// `Object` in a class deriving from its owner, which dotty rejects.
pub fn super_accessor_needed(u: &mut Universe, sym: SymbolId, class: SymbolId) -> Result<bool, &'static str> {
    if !needs_accessor_if_not_in_subclass(u, sym, class)
        || !host_for_accessor_of(u, sym, class).is_some_and(|h| u.is_trait(h))
    {
        return Ok(false);
    }
    let owner = u.symbol(sym).owner.expect("a member's owner");
    if !u.derives_from(class, owner) {
        return Ok(true);
    }
    match is_jdk(u, owner) && u.full_name(owner) != "java.lang.Object" {
        true => Err("protected Java members on `this` in concrete trait methods"),
        false => Ok(false),
    }
}

/// The closest class enclosing `from` that derives from the owner of `sym` or whose object owns
/// it (dotty's `hostForAccessorOf`).
pub fn host_for_accessor_of(u: &mut Universe, sym: SymbolId, from: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(sym).owner?;
    let companion = u.is_module_class(owner).then(|| u.companion_class(owner)).flatten();
    let mut c = from;
    loop {
        if u.is_class(c) && (u.derives_from(c, owner) || Some(c) == companion) {
            return Some(c);
        }
        c = u.symbol(c).owner?;
    }
}

/// Java's `static`, which `@main` proxies carry; the methods and values of javalib's objects
/// that stand for the static members of JDK classes; and the library's `@static` members.
pub fn is_java_static(u: &mut Universe, s: SymbolId) -> bool {
    let member = matches!(u.symbol(s).kind, Kind::Def | Kind::Val) && !u.is_module(s);
    u.symbol(s).flags.has(STATIC)
        || (member && u.symbol(s).owner.is_some_and(|o| is_jdk_object(u, o)))
        || (u.symbol(s).tree.is_some() && u.is_annotated(s, "scala.annotation.static"))
}

/// Whether class `c` is the class of a javalib object in a JDK package or of an object of the JDK
/// jars that stands for a JDK class, or of a Java class's stub object.
pub fn is_jdk_object(u: &Universe, c: SymbolId) -> bool {
    let name = u.full_name(c);
    let jdk = name.starts_with("java.") || name.starts_with("javax.") || u.is_from_jdk_jars(c);
    u.may_be_java(c) && u.is_module_class(c) && jdk || u.is_java_statics(c)
}

/// Whether class `c` is a JDK class: Java-defined in a `java.` or `javax.` package.
pub fn is_jdk(u: &Universe, c: SymbolId) -> bool {
    let name = u.full_name(c);
    u.may_be_java(c) && (name.starts_with("java.") || name.starts_with("javax."))
}

/// Whether `s` is `c` or defined in it.
pub fn within(u: &Universe, mut s: SymbolId, c: SymbolId) -> bool {
    loop {
        if s == c {
            return true;
        }
        let Some(o) = u.symbol(s).owner else { return false };
        s = o;
    }
}

/// The package `s` is in.
fn package_of(u: &Universe, mut s: SymbolId) -> SymbolId {
    while u.symbol(s).kind != Kind::Package {
        s = u.symbol(s).owner.expect("an owner up to a package");
    }
    s
}
