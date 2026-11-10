//! What the lowering asks of symbols (dotty's `SymDenotations` predicates).

mod jdk_17_missing;
mod jdk_inherited;

use tasty::tags::{ERASED, EXTENSION, FINAL, IMPLICIT, LAZY, PRIVATE, STATIC};
use tasty::{Kind, SymbolId, Universe};

pub fn has(u: &Universe, s: SymbolId, tag: u8) -> bool {
    u.symbol(s).flags.has(tag)
}

pub fn is_private(u: &Universe, s: SymbolId) -> bool {
    has(u, s, PRIVATE)
}

/// Whether term `s` is erased: flagged so, or `erasedValue` of `scala.compiletime` or
/// `unsafeErasedValue` of `scala.caps` (dotty's `isErased`).
pub fn is_erased(u: &Universe, s: SymbolId) -> bool {
    let sym = u.symbol(s);
    if !sym.kind.is_term() {
        return false;
    }
    let full = u.full_name(s);
    has(u, s, ERASED)
        || matches!(sym.name.as_str(), "erasedValue" | "unsafeErasedValue")
            && (full.starts_with("scala.compiletime.") || full.starts_with("scala.caps."))
}

/// Whether `tpe` is the type of a single value or an object's class, which `null` does not
/// conform to (dotty's `isNullableClass`).
pub fn is_singleton(u: &mut Universe, tpe: tasty::TypeId) -> bool {
    match *u.ty(tpe) {
        tasty::Type::Sym(_, s) if u.symbol(s).kind.is_term() => true,
        tasty::Type::TermRef(..) => true,
        _ => u.class_symbol(tpe).is_some_and(|c| u.is_module_class(c)),
    }
}

/// Whether member `s` of class `class` cannot be overridden.
pub fn is_effectively_final(u: &Universe, class: SymbolId, s: SymbolId) -> bool {
    has(u, s, PRIVATE) || has(u, s, FINAL) || has(u, class, FINAL) || u.is_module_class(class)
}

pub use typer::protected_access::{is_java_static, is_jdk_object};

/// Whether `s` is a Java field: one of a JDK stub or a JDK jar, or a value javalib's class (not
/// trait) in a JDK package declares outside private and lazy ones, which dotty reads from the
/// JDK's class; not one of Scala Native's renamed classes such as `_Class`.
pub fn is_java_field(u: &Universe, s: SymbolId) -> bool {
    let jdk_class = |c| {
        let name = u.full_name(c);
        let jdk = name.starts_with("java.") || name.starts_with("javax.");
        jdk && u.may_be_java(c)
            && !u.is_module_class(c)
            && !u.is_trait(c)
            && tasty::defined_name(&name).is_none()
    };
    let javalib = u.symbol(s).kind == Kind::Val
        && ![PRIVATE, STATIC, LAZY].iter().any(|&f| has(u, s, f))
        && !u.is_module(s)
        && u.symbol(s).owner.is_some_and(jdk_class);
    javalib || u.is_java_field(s)
}

/// Whether the owner of `s` is a static owner.
pub fn is_static(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).owner.is_some_and(|o| u.is_static_owner(o))
}

pub const EXTERN: &str = "scala.scalanative.unsafe.package$.extern";

/// Whether `s` or its owner is Scala Native's `@extern`, unless `s` is not extern: an extension
/// method or implicit class conversion of an extern object (nscplugin's `PrepNativeInterop`).
pub fn is_extern(u: &mut Universe, s: SymbolId) -> bool {
    let annotated = |u: &Universe, s: SymbolId| {
        u.symbol(s).is_extern || u.annotations(s).iter().any(|&c| u.full_name(c) == EXTERN)
    };
    let in_extern = u.symbol(s).owner.is_some_and(|o| annotated(u, o));
    if !annotated(u, s) && !in_extern {
        return false;
    }
    let prep_non_extern =
        u.symbol(s).tree.is_none() && in_extern && (has(u, s, EXTENSION) || is_implicit_class_ctor(u, s));
    !(prep_non_extern || u.is_annotated(s, "scala.scalanative.annotation.nonExtern"))
}

/// Whether `m` is the implicit conversion of an implicit class beside it.
fn is_implicit_class_ctor(u: &Universe, m: SymbolId) -> bool {
    let Some(owner) = u.symbol(m).owner.filter(|_| has(u, m, IMPLICIT)) else { return false };
    let name = &u.symbol(m).name;
    u.symbol(owner).decls.iter().any(|&c| u.is_class(c) && has(u, c, IMPLICIT) && u.symbol(c).name == *name)
}

/// The javalib classes that JDK classes extend and the JDK does not, each with the JDK class
/// and the members of it in the JDK API that javalib defines only there.
const JAVALIB_ONLY_MEMBERS: [(&str, &str, &[&str]); 1] =
    [("scala.scalanative.runtime.Throwable", "java.lang.Throwable", &["setStackTrace"])];

/// The JDK class whose member `m` is in the JDK API, if javalib defines it in a class of its own
/// the JDK class extends, or in a JDK class that inherits it in the JDK, as Scala Native,
/// compiling against the JDK API, refers to it.
pub fn jdk_api_owner(u: &Universe, m: SymbolId) -> Option<&'static str> {
    let s = u.symbol(m);
    let class = s.owner?;
    let owner = u.full_name(class);
    let name = s.name.as_str();
    if owner.starts_with("java") {
        let found =
            jdk_inherited::INHERITED.binary_search_by(|&(c, n, _)| (c, n).cmp(&(owner.as_str(), name)));
        return found.ok().filter(|_| u.may_be_java(class)).map(|i| jdk_inherited::INHERITED[i].2);
    }
    JAVALIB_ONLY_MEMBERS.iter().find(|e| e.0 == owner && e.2.contains(&name)).map(|&(_, jdk, _)| jdk)
}

/// Whether the compile sees the JDK 17 API and member `m` of a javalib class is not in it.
pub fn beyond_jdk_view(u: &Universe, m: SymbolId) -> bool {
    let Some(class) = u.symbol(m).owner.filter(|&c| u.jdk_17_view() && u.may_be_java(c)) else {
        return false;
    };
    let owner = u.full_name(class);
    let name = u.symbol(m).name.as_str();
    jdk_17_missing::MISSING.binary_search(&(owner.as_str(), name)).is_ok()
}
