// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! Erasure's casts: where a reference's erased class does not conform to the one expected, and
//! the class a call's receiver is adapted to.

use super::adapt::from_start;
use super::function_classes::{erased_derives, erased_function_class};
use crate::Lower;
use crate::definitions::type_ref;
use crate::symbols::has;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use tasty::{Erased, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `t.asInstanceOf[T]` for the type `T` erasing to `to`, a primitive array first made an array of
/// references by `ScalaRunTime.toObjectArray` where one is expected (dotty's `Erasure.Boxing.cast`).
pub fn cast(l: &mut Lower, t: TNodeId, to: &Erased) -> TNodeId {
    let prim = |e: &Erased| matches!(e, Erased::Prim(_));
    let t = match (super::adapt::erased_type(l, t), to) {
        (Erased::Array(e), Erased::Array(p)) if prim(&e) && !prim(p) => {
            let module = l.scala_run_time().expect("`ScalaRunTime`");
            let class = l.u.module_class(module).expect("ScalaRunTime's class");
            let method = l.u.decl(class, "toObjectArray").unwrap_or_default();
            let method = *method.first().expect("`ScalaRunTime.toObjectArray`");
            super::adapt::converted(l, t, module, method)
        }
        _ => t,
    };
    let span = from_start(l.tpd.span(t));
    let target = type_of(l, to);
    let fun = l.select(t, l.defs.as_instance_of, span);
    let tpt = l.type_tree(target, span);
    l.build().type_apply(fun, &[tpt], target, span)
}

/// A type erasing to `e`.
pub fn type_of(l: &mut Lower, e: &Erased) -> TypeId {
    match e {
        Erased::Array(elem) => {
            let elem = type_of(l, elem);
            let array = l.u.class_ref("scala.Array");
            l.u.intern_type(Type::Applied(array, vec![elem]))
        }
        Erased::Prim(c) => {
            let p = l.primitive(*c).expect("a primitive letter").module;
            let class = l.u.module_class(p).expect("a primitive's object class");
            let name = l.u.full_name(class);
            l.u.class_ref(name.trim_end_matches('$'))
        }
        Erased::Class(name) => match class_named(l, name) {
            Some(c) => type_ref(l.u, c),
            None => l.u.class_ref(name),
        },
        Erased::Nothing => l.u.class_ref("scala.Nothing"),
        _ => l.u.class_ref("java.lang.Object"),
    }
}

/// Whether a value of erased type `from` is a value of erased reference type `to`.
pub fn conforms(l: &mut Lower, from: &Erased, to: &Erased) -> bool {
    match (from, to) {
        (_, Erased::Class(o)) if o == "java.lang.Object" => true,
        _ if from == to => true,
        (Erased::Array(x), Erased::Array(y)) => {
            matches!(**y, Erased::Class(ref o) if o == "java.lang.Object")
                && matches!(**x, Erased::Class(_) | Erased::Array(_))
        }
        (Erased::Class(f), Erased::Class(t)) => match (class_named(l, f), class_named(l, t)) {
            (Some(f), Some(t)) => {
                erased_derives(l, f, t) && (!l.u.is_trait(f) || erased_trait_derives(l, f, t))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether trait `f` is or derives from `t` through its trait parents, the only ones erasure
/// keeps besides `Object` (dotty's `TypeErasure.eraseInfo`).
fn erased_trait_derives(l: &mut Lower, f: SymbolId, t: SymbolId) -> bool {
    if f == t || erased_function_class(l, f) == Some(t) {
        return true;
    }
    let parents: Vec<SymbolId> = l.u.parents(f).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    parents.into_iter().any(|p| l.u.is_trait(p) && erased_derives(l, p, t) && erased_trait_derives(l, p, t))
}

/// The class of binary name `name`, through the classes and objects it is nested in.
pub fn class_named(l: &mut Lower, name: &str) -> Option<SymbolId> {
    let (pkg, rest) = match name.rsplit_once('.') {
        Some((pkg, rest)) => (l.u.package(pkg)?, rest),
        None => (l.u.package("")?, name),
    };
    if let Some((prefix, _)) = rest.split_once("$$$$") {
        let tops = l.u.symbol(pkg).decls.clone().into_iter().filter(|&c| l.u.is_class(c));
        let tops: Vec<SymbolId> = tops
            .filter(|&c| {
                let n = tasty::encode(&l.u.symbol(c).name);
                prefix.starts_with(&n) || n.starts_with(prefix)
            })
            .collect();
        return tops.into_iter().find_map(|c| compacted_member(l, c, name));
    }
    let found = member_named(l, pkg, rest)?;
    (l.u.binary_name(found) == name).then_some(found)
}

/// The class of `compactify`d binary name `name` among `class` and the classes nested in it.
fn compacted_member(l: &mut Lower, class: SymbolId, name: &str) -> Option<SymbolId> {
    if l.u.binary_name(class) == name {
        return Some(class);
    }
    let nested: Vec<SymbolId> =
        l.u.symbol(class).decls.iter().copied().filter(|&c| l.u.is_class(c)).collect();
    nested.into_iter().find_map(|c| compacted_member(l, c, name))
}

/// The class of binary name `rest` relative to `owner`: a member class `C`, or a class nested in
/// member class or object `C` named `C$rest`; operator names encoded.
fn member_named(l: &mut Lower, owner: SymbolId, rest: &str) -> Option<SymbolId> {
    let classes = |l: &mut Lower, owner: SymbolId, name: &str| -> Vec<SymbolId> {
        let mut found = l.u.decl(owner, name).unwrap_or_default();
        let decoded = tasty::decode(name);
        if found.is_empty() && decoded != name {
            found = l.u.decl(owner, &decoded).unwrap_or_default();
        }
        found.into_iter().filter(|&s| l.u.is_class(s)).collect()
    };
    if let Some(&c) = classes(l, owner, rest).first() {
        return Some(c);
    }
    for (i, _) in rest.match_indices('$').filter(|&(i, _)| i > 0) {
        for (name, tail) in [(&rest[..i], &rest[i + 1..]), (&rest[..=i], &rest[i + 1..])] {
            for c in classes(l, owner, name) {
                if tail.is_empty() {
                    return Some(c);
                }
                if let Some(found) = member_named(l, c, tail) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Whether `t` is a cast `x.asInstanceOf[T]`.
pub fn is_cast(l: &Lower, t: TNodeId) -> bool {
    matches!(*l.tpd.tree(t), Tree::TypeApply { fun, .. } if l.tpd.sym(fun) == Some(l.defs.as_instance_of))
}

/// The erased type the receiver `qualifier` of selection `sel` is adapted to: the class owning
/// the selected member, or the receiver's own type if that is a JDK class that is not public
/// (dotty's `isJvmAccessible`); `None` for a `super` receiver, a member of `Array` or a member
/// outside classes.
pub fn receiver_type(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Option<Erased> {
    if matches!(l.tpd.tree(qualifier), Tree::Super { .. }) {
        return None;
    }
    let owner = l.u.symbol(l.tpd.sym(sel)?).owner.filter(|&o| l.u.is_class(o))?;
    let name = l.u.full_name(owner);
    if name == "scala.Array" {
        let array = l.u.widen(l.tpd.tpe(qualifier));
        let e = l.u.erase(array);
        return matches!(e, Erased::Array(_)).then_some(e);
    }
    let jdk = name.starts_with("java.") || name.starts_with("javax.");
    let hidden =
        [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&f| has(l.u, owner, f));
    let tpe = if jdk && hidden { l.u.widen(l.tpd.tpe(qualifier)) } else { type_ref(l.u, owner) };
    Some(l.u.erase_boxed(tpe))
}
