//! Whether a type test can hold (dotty's `TypeTestsCasts.checkSensical`): not if the tested
//! value's class is final and no subclass of the tested class, or the two are unrelated classes.

use super::casts::class_named;
use crate::Lower;
use crate::definitions::primitive_class_name;
use crate::symbols::has;
use tasty::tags::FINAL;
use tasty::{Erased, SymbolId, Type, TypeId};

/// Whether a value of type `tpe` can be an instance of erased class `test`, the box class of
/// primitive `unboxed` if they differ.
pub fn sensical(l: &mut Lower, tpe: TypeId, test: &Erased, unboxed: &Erased) -> bool {
    let Some(test) = class_of(l, test) else { return true };
    let unboxed = class_of(l, unboxed).unwrap_or(test);
    let found = found_classes(l, tpe);
    found.is_empty() || found.into_iter().any(|f| check(l, f, test, unboxed))
}

fn check(l: &mut Lower, found: SymbolId, test: SymbolId, unboxed: SymbolId) -> bool {
    let b = l.u.builtins();
    if [b.any, b.object, b.any_ref, b.matchable].contains(&found) {
        return true;
    }
    if !checkable(l, found, test) || l.u.derives_from(found, test) {
        return true;
    }
    let final_found = has(l.u, found, FINAL);
    let unrelated = !l.u.derives_from(test, found)
        && !l.u.derives_from(unboxed, found)
        && (has(l.u, test, FINAL) || !l.u.is_trait(test) && !l.u.is_trait(found));
    !final_found && !unrelated
}

/// Whether tests of values of class `found` for class `test` can be decided statically.
fn checkable(l: &mut Lower, found: SymbolId, test: SymbolId) -> bool {
    let (found_prim, test_prim) = (is_primitive(l, found), is_primitive(l, test));
    found_prim == test_prim && !is_value_class(l, found) && !is_value_class(l, test)
}

/// The classes of the values of type `tpe`, `AnyVal` as `Any` and `*:` as its erasure's; none
/// if unknown (dotty's `effectiveClass`).
fn found_classes(l: &mut Lower, tpe: TypeId) -> Vec<SymbolId> {
    let tpe = l.u.widen(tpe);
    let tpe = l.u.dealias(tpe);
    match *l.u.types().get(tpe) {
        Type::Or(a, b, _) => {
            let mut out = found_classes(l, a);
            out.extend(found_classes(l, b));
            out
        }
        Type::And(..) => Vec::new(),
        _ => {
            let mut class = l.u.class_symbol(tpe);
            if class.is_some_and(|c| l.u.full_name(c) == "scala.*:") {
                class = match l.u.erase(tpe) {
                    Erased::Class(name) => class_named(l, &name),
                    _ => None,
                };
            }
            let any_val = l.u.builtins().any_val;
            class.map(|c| if Some(c) == any_val { l.u.builtins().any } else { c }).into_iter().collect()
        }
    }
}

fn class_of(l: &mut Lower, e: &Erased) -> Option<SymbolId> {
    match e {
        Erased::Prim(c) => class_named(l, &primitive_class_name(*c)?),
        Erased::Class(name) => class_named(l, name),
        _ => None,
    }
}

fn is_primitive(l: &Lower, c: SymbolId) -> bool {
    let name = l.u.full_name(c);
    name == "scala.Unit"
        || "zcbsijfd".chars().any(|p| primitive_class_name(p).as_deref() == Some(name.as_str()))
}

/// Whether `c` is a user value class (a subclass of `AnyVal` that is not primitive).
fn is_value_class(l: &mut Lower, c: SymbolId) -> bool {
    let Some(any_val) = l.u.builtins().any_val else { return false };
    c != any_val && l.u.derives_from(c, any_val) && !is_primitive(l, c)
}
