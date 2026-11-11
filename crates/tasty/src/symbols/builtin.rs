// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! The classes dotty defines without TASTy (`Definitions`): `Any`, `AnyKind`, `Matchable`,
//! `Nothing`, `Null`, `Singleton`, the aliases `AnyRef` and `<FromJavaObject>`, and `java.lang.Object` where no library
//! defines it; `Object` extends `Any` and `Matchable`, `java.io.Serializable` and
//! `java.lang.Cloneable` only `Any`.

use super::{Defn, Flags, Kind, Symbol, SymbolId, Universe};
use crate::tags::{ABSTRACT, FINAL, PRIVATE, PRIVATEqualified, TRAIT};
use crate::types::{Type, TypeId};
use names::nme;

/// The type `scala.Nothing`, the first type every universe interns.
pub const NOTHING: TypeId = TypeId(0);
/// The type `scala.Any`, the second.
pub const ANY: TypeId = TypeId(1);

/// A symbol of `kind` without TASTy, flagged with the modifier tags `flags`.
pub fn synthetic(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind, flags: &[u8]) -> SymbolId {
    let flags = flags.iter().fold(Flags::default(), |f, &t| f.with(t));
    u.define(Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    })
}

/// Enters the classes without TASTy into `u`, whose libraries' packages are known and which has
/// interned no type yet, and returns the well-known symbols they make.
pub fn enter(u: &mut Universe) -> Defn {
    let scala = u.enter_package("scala");
    let nothing = synthetic(u, scala, "Nothing", Kind::Class, &[ABSTRACT, FINAL]);
    let any = synthetic(u, scala, "Any", Kind::Class, &[ABSTRACT]);
    assert_eq!(u.types.intern(Type::Sym(None, nothing)), NOTHING);
    assert_eq!(u.types.intern(Type::Sym(None, any)), ANY);
    let any_kind = synthetic(u, scala, "AnyKind", Kind::Class, &[ABSTRACT, FINAL]);
    let matchable = synthetic(u, scala, "Matchable", Kind::Class, &[TRAIT]);
    let null = synthetic(u, scala, "Null", Kind::Class, &[ABSTRACT, FINAL]);
    let singleton = synthetic(u, scala, "Singleton", Kind::Class, &[TRAIT]);
    u.parents.insert(matchable, vec![ANY]);
    u.parents.insert(singleton, vec![ANY]);
    let java_lang = u.enter_package("java.lang");
    let defined = u.pending.get(&java_lang).is_some_and(|f| f.iter().any(|(stem, _)| stem == "Object"));
    let object = match defined {
        true => u
            .decl(java_lang, "Object")
            .ok()
            .and_then(|d| d.into_iter().find(|&s| u.symbol(s).kind == Kind::Class)),
        false => None,
    };
    let object = object.unwrap_or_else(|| synthetic(u, java_lang, "Object", Kind::Class, &[]));
    let flags = &mut u.symbols[object.0 as usize].flags;
    *flags = flags.without(PRIVATE).without(PRIVATEqualified);
    let matchable_ref = u.types.intern(Type::Sym(None, matchable));
    u.parents.insert(object, vec![ANY, matchable_ref]);
    let object_ref = u.types.intern(Type::Sym(None, object));
    u.parents.insert(nothing, vec![ANY]);
    u.parents.insert(null, vec![object_ref]);
    for (pkg, name) in [("java.io", "Serializable"), ("java.lang", "Cloneable")] {
        let universal = u
            .package(pkg)
            .and_then(|p| u.decl(p, name).ok()?.into_iter().find(|&s| u.symbol(s).kind == Kind::Class));
        if let Some(universal) = universal {
            u.parents.insert(universal, vec![ANY]);
        }
    }
    let any_ref = synthetic(u, scala, "AnyRef", Kind::Type, &[]);
    u.infos.insert(any_ref, object_ref);
    let ops = u.enter_package(nme::OPS_PACKAGE);
    let from_java_object = synthetic(u, ops, "<FromJavaObject>", Kind::Type, &[]);
    u.infos.insert(from_java_object, object_ref);
    let any_val =
        u.decl(scala, "AnyVal").ok().and_then(|d| d.into_iter().find(|&s| u.symbol(s).kind == Kind::Class));
    super::any_members::enter(u, any, object);
    super::impure_functions::enter(u, scala);
    super::function_classes::enter(u, scala, object_ref);
    super::java_stubs::enter(u, object);
    Defn {
        scala,
        java_lang,
        any,
        any_kind,
        matchable,
        nothing,
        null,
        singleton,
        any_ref,
        object,
        from_java_object,
        any_val,
        ..Defn::PENDING
    }
}
