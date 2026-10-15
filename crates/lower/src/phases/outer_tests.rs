//! The tests of enclosing instances in type patterns (dotty's `PatternMatcher.addOuterTest`).

use super::explicit_outer::enclosing_instance;
use super::local_free::is_local;
use super::outer_traits::outer_class;
use crate::Lower;
use crate::symbols::is_static;
use tasty::{Type, TypeId};

/// Why a type test against `target` cannot be lowered: an object of a class, or in a match a
/// class that may test its enclosing instance.
pub fn unsupported_test(l: &mut Lower, target: TypeId, in_match: bool) -> Option<&'static str> {
    if let Type::Sym(_, s) = *l.u.types().get(target)
        && l.u.is_module(s)
        && !is_static(l.u, s)
    {
        return Some("type tests of objects nested in classes");
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    let inner = l.u.class_symbol(dealiased).is_some_and(|c| outer_class(l, c).is_some() && !is_local(l, c));
    let tested = outer_test(l, target).is_some();
    (in_match && inner && !tested).then_some("patterns of classes in classes")
}

/// What a match tests of the enclosing instance of a value of type `p.C`.
pub enum OuterTest {
    /// It is `p`, a singleton.
    Is(TypeId),
    /// It has class type `p`.
    Has(TypeId),
    /// Nothing: the owner of `C` derives from the class of `p`.
    Nothing,
}

/// The test of the enclosing instance of a value of type `p.C` of a class `C`, `None` if `C` has
/// no enclosing instance or `p` is neither a singleton nor a class type.
pub fn outer_test(l: &mut Lower, target: TypeId) -> Option<OuterTest> {
    let target = l.u.dealias(target);
    let target = match *l.u.types().get(target) {
        Type::Applied(tycon, _) => tycon,
        _ => target,
    };
    let Type::Sym(Some(pre), c) = *l.u.types().get(target) else { return None };
    if !l.u.is_class(c) || enclosing_instance(l, c).is_none() {
        return None;
    }
    if super::type_tests::singletons::is_singleton(l, pre) {
        return Some(OuterTest::Is(pre));
    }
    let base = l.u.class_symbol(pre)?;
    let owner = l.u.symbol(c).owner.expect("a class's owner");
    Some(match l.u.derives_from(owner, base) {
        true => OuterTest::Nothing,
        false => OuterTest::Has(pre),
    })
}
