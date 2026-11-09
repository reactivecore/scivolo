// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//   compiler/src/dotty/tools/dotc/transform/ProtectedAccessors.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! Whether `C.this.m` of a protected `m` gets a super accessor of trait `C` (`transformSelect`).

use super::{Refusal, TreePickler};
use tasty::tags::{PROTECTED, PROTECTEDqualified, STATIC};
use tasty::{Kind, SymbolId, Universe};

impl TreePickler<'_> {
    /// Whether `sym` selected on `this` from class `class` needs a super accessor in the trait
    /// hosting its accessor; refused for a protected JDK member in a class deriving from its owner.
    pub fn protected_super_needed(&mut self, sym: SymbolId, class: SymbolId) -> Result<bool, Refusal> {
        let u = &mut *self.u;
        if !needs_accessor_if_not_in_subclass(u, sym, class) || !host_for_accessor_of(u, sym, class).is_some_and(|h| u.is_trait(h)) {
            return Ok(false);
        }
        let owner = u.symbol(sym).owner.ok_or("a member without owner")?;
        if !u.derives_from(class, owner) {
            return Ok(true);
        }
        let name = u.full_name(owner);
        match u.may_be_java(owner) && (name.starts_with("java.") || name.starts_with("javax.")) && name != "java.lang.Object" {
            true => Err("protected Java members on `this` in concrete trait methods"),
            false => Ok(false),
        }
    }
}

/// dotty's `needsAccessorIfNotInSubclass`.
fn needs_accessor_if_not_in_subclass(u: &mut Universe, sym: SymbolId, class: SymbolId) -> bool {
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

/// Java's `static`, the members of javalib's objects that stand for a JDK class's statics, and
/// `@static` members.
fn is_java_static(u: &mut Universe, s: SymbolId) -> bool {
    let jdk_object = |u: &Universe, c| {
        let name = u.full_name(c);
        let jdk = name.starts_with("java.") || name.starts_with("javax.") || u.is_from_jdk_jars(c);
        u.may_be_java(c) && u.is_module_class(c) && jdk || u.is_java_statics(c)
    };
    let member = matches!(u.symbol(s).kind, Kind::Def | Kind::Val) && !u.is_module(s);
    u.symbol(s).flags.has(STATIC)
        || (member && u.symbol(s).owner.is_some_and(|o| jdk_object(u, o)))
        || (u.symbol(s).tree.is_some() && u.is_annotated(s, "scala.annotation.static"))
}

/// The closest class enclosing `from` that derives from the owner of `sym` or whose object owns
/// it (dotty's `hostForAccessorOf`).
fn host_for_accessor_of(u: &mut Universe, sym: SymbolId, from: SymbolId) -> Option<SymbolId> {
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

fn package_of(u: &Universe, mut s: SymbolId) -> SymbolId {
    while let Some(o) = u.symbol(s).owner.filter(|_| u.symbol(s).kind != Kind::Package) {
        s = o;
    }
    s
}

/// Whether `s` is `c` or defined in it.
fn within(u: &Universe, mut s: SymbolId, c: SymbolId) -> bool {
    loop {
        if s == c {
            return true;
        }
        let Some(o) = u.symbol(s).owner else { return false };
        s = o;
    }
}
