//! dotty's `ExplicitOuter`: which classes and traits hold their enclosing instance, the outer
//! arguments of their constructor calls, then the paths to enclosing instances
//! (`outer_fields.rs`, `outer_traits.rs`).

use super::local_free::is_term;
use super::outer_traits::outer_class;
use super::references_outer::references_outer;
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_static};
use crate::{Lower, Result};
use tasty::tags::{FINAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    let mut mixes_inner = Vec::new();
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        mixes_inner.push(super::outer_traits::implement(l, td, class)?);
    }
    let mut with_outer = Vec::new();
    for (&td, mixes_inner) in classes.iter().zip(mixes_inner) {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if let Some(outer) = super::local_outer::local_outer(l, td, class) {
            super::outer_fields::declare(l, td, class, outer);
            with_outer.push((td, class));
            continue;
        }
        let Some(outer) = outer_class(l, class) else { continue };
        let uses = references_outer(l, class, td);
        let inner_parent = l.u.is_trait(class) && super::outer_traits::has_inner_parent(l, class);
        let always = mixes_inner
            || inner_parent
            || match l.u.is_module_class(class) {
                true => l.u.is_trait(outer),
                false => !has(l.u, class, PRIVATE) && !is_local(l, outer),
            };
        if always || uses || l.outer_accessors.contains_key(&class) {
            if !l.u.is_trait(class) {
                super::outer_fields::declare(l, td, class, outer);
            }
            with_outer.push((td, class));
        }
    }
    for &td in classes {
        super::outer_args::pass_outer(l, td)?;
    }
    for &(td, class) in with_outer.iter().rev() {
        match l.u.is_trait(class) {
            true => super::outer_traits::add_outer(l, td, class),
            false => super::outer_fields::rewrite(l, td, class)?,
        }
    }
    Ok(())
}

/// The class whose instance a class or object `class` nested in it can use (dotty's
/// `needsOuterIfReferenced`), unless static or a trait.
pub fn enclosing_instance(l: &Lower, class: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(class).owner?;
    let nested = l.u.is_class(owner) && !l.u.is_trait(class);
    (nested && !is_static(l.u, class)).then_some(owner)
}

/// Whether `class` is defined in a method or value, directly or through other classes.
fn is_local(l: &Lower, class: SymbolId) -> bool {
    let mut owner = l.u.symbol(class).owner;
    while let Some(o) = owner {
        if l.u.symbol(o).kind.is_term() {
            return true;
        }
        owner = l.u.symbol(o).owner;
    }
    false
}

/// Why a type test against `target` cannot be lowered: an object of a class, or in a match a
/// class that may test its enclosing instance (dotty's `addOuterTest`).
pub fn unsupported_test(l: &mut Lower, target: tasty::TypeId, in_match: bool) -> Option<&'static str> {
    if let Type::Sym(_, s) = *l.u.types().get(target)
        && l.u.is_module(s)
        && !is_static(l.u, s)
    {
        return Some("type tests of objects nested in classes");
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    let inner = l.u.class_symbol(dealiased).is_some_and(|c| outer_class(l, c).is_some());
    let tested = outer_prefix(l, target).is_some();
    (in_match && inner && !tested).then_some("patterns of classes in classes")
}

/// The prefix `O.this` of type `O.this.C` of a class `C` with an enclosing instance, the instance
/// a match tests (dotty's `addOuterTest` for a singleton prefix).
pub fn outer_prefix(l: &mut Lower, target: tasty::TypeId) -> Option<tasty::TypeId> {
    let target = l.u.dealias(target);
    let target = match *l.u.types().get(target) {
        Type::Applied(tycon, _) => tycon,
        _ => target,
    };
    let Type::Sym(Some(pre), c) = *l.u.types().get(target) else { return None };
    let singleton = matches!(l.u.types().get(pre), Type::This(_));
    (singleton && l.u.is_class(c) && enclosing_instance(l, c).is_some()).then_some(pre)
}

/// `t` and the trees in it but those in type trees, and in class definitions within `t` unless
/// `nested`.
pub fn terms(l: &Lower, t: TNodeId, nested: bool) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![t];
    while let Some(s) = todo.pop() {
        out.push(s);
        let skip = match l.tpd.tree(s) {
            Tree::SingletonTypeTree { .. } | Tree::AppliedTypeTree { .. } | Tree::TypeTree { .. } => true,
            Tree::TypeDef { .. } => s != t && !nested,
            _ => false,
        };
        if !skip {
            todo.extend(super::walk::children(l.tpd, s));
        }
    }
    out
}

/// The classes from the enclosing instance of `class` out to the one `t` is `this` of, if `t`
/// is `this` of a class enclosing `class` through enclosing instances (dotty's `outer.path`).
pub fn outer_this(l: &mut Lower, t: TNodeId, class: SymbolId) -> Option<Vec<SymbolId>> {
    let target = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::This { .. }))?;
    outer_chain(l, class, target)
}

/// The classes from the enclosing instance of `class` out to `target`, if it is reached through
/// enclosing instances.
pub fn outer_chain(l: &mut Lower, class: SymbolId, target: SymbolId) -> Option<Vec<SymbolId>> {
    let mut chain = Vec::new();
    let mut cur = instance_class(l, class);
    while let Some(c) = cur {
        chain.push(c);
        if c == target {
            return Some(chain);
        }
        cur = instance_class(l, c);
    }
    None
}

/// The class whose instance class `c` holds or, if not yet decided, may hold.
fn instance_class(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    if let Some(&(_, param)) = l.outers.get(&c) {
        let info = l.u.info(param);
        let Type::Sym(_, outer) = *l.u.types().get(info) else { unreachable!("a class type") };
        return Some(outer);
    }
    let local = l.u.symbol(c).owner.is_some_and(|o| is_term(l, o));
    if local { None } else { outer_class(l, c) }
}

/// The accessor `C$$$outer()` of the enclosing instance of class `class`, defined once; `class`
/// then gets its field (dotty's `outerAccName`, `ensureOuterAccessors`).
pub fn outer_accessor(l: &mut Lower, class: SymbolId) -> SymbolId {
    if let Some(&m) = l.outer_accessors.get(&class) {
        return m;
    }
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    let (name, outer) = match super::local_free::is_term(l, owner) {
        true => (
            format!("{}$$$outer", super::local_outer::accessor_prefix(l, class)),
            super::local_free::enclosing_class(l, owner),
        ),
        false => (
            outer_accessor_name(l, class),
            outer_class(l, class).expect("a class with an enclosing instance"),
        ),
    };
    let m = l.u.define(symbol(&name, Kind::Def, class, Flags::default().with(FINAL)));
    let result = type_ref(l.u, outer);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(m, info);
    l.outer_accessors.insert(class, m);
    m
}

/// `C$$$outer`, the name of the accessor of the enclosing instance of class `class`.
pub fn outer_accessor_name(l: &Lower, class: SymbolId) -> String {
    format!("{}$$$outer", l.u.binary_name(class).replace('.', "$"))
}
