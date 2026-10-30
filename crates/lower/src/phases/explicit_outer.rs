// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/transform/ExplicitOuter.scala
//! dotty's `ExplicitOuter`: which classes and traits hold their enclosing instance, the outer
//! arguments of their constructor calls, then the paths to enclosing instances
//! (`outer_fields.rs`, `outer_traits.rs`).

use super::local_free::{is_local, is_term};
use super::outer_traits::outer_class;
use super::references_outer::references_outer;
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_static};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{FINAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
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
        if is_local(l, class) {
            if let Some(outer) = super::local_outer::local_outer(l, td, class) {
                if !l.u.is_trait(class) {
                    super::outer_fields::declare(l, td, class, outer);
                }
                with_outer.push((td, class));
            }
            continue;
        }
        let Some(outer) = outer_class(l, class) else { continue };
        let uses = references_outer(l, class, td);
        let inner_parent = super::outer_traits::has_inner_parent(l, class);
        let always = mixes_inner
            || inner_parent
            || match l.u.is_module_class(class) {
                true => l.u.is_trait(outer),
                false => !has(l.u, class, PRIVATE) && !in_term(l, outer),
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
    super::outer_selects::transform(l, classes);
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
pub fn in_term(l: &Lower, class: SymbolId) -> bool {
    let mut owner = l.u.symbol(class).owner;
    while let Some(o) = owner {
        if l.u.symbol(o).kind.is_term() {
            return true;
        }
        owner = l.u.symbol(o).owner;
    }
    false
}

/// `t` and the trees in it but those in type trees, and in class definitions within `t` unless
/// `nested`.
pub fn terms(l: &Lower, t: TNodeId, nested: bool) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![t];
    while let Some(s) = todo.pop() {
        out.push(s);
        let skip = match l.tpd.tree(s) {
            _ if super::walk::is_type_tree(l, s) => true,
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

/// For a reference `t` by name to a member of a class enclosing `class` through enclosing
/// instances, or of objects in it, `t` made a selection on that path, whose `this` is returned
/// (dotty's `desugarIdent`); of an object `class` is in, its `this`.
pub fn outer_prefix(l: &mut Lower, t: TNodeId, class: SymbolId) -> Option<TNodeId> {
    let Tree::Ident { name } = *l.tpd.tree(t) else { return None };
    let Type::Sym(Some(prefix), s) = *l.u.types().get(l.tpd.get(t).tpe?) else { return None };
    if !l.u.symbol(s).kind.is_term() {
        return None;
    }
    let at = l.tpd.span(t).start_pos();
    let (qualifier, this) = prefix_path(l, prefix, class, at)?;
    let module = l.tpd.sym(t).and_then(|s| l.u.module_class(s).filter(|_| l.u.is_module(s)));
    if let Some(m) = module.filter(|&m| qualifier == this && super::local_free::within(l, class, m)) {
        let this = l.this(m, l.tpd.span(t));
        l.copy_node(t, this);
        return Some(t);
    }
    l.tpd.get_mut(t).tree = Tree::Select { qualifier, name };
    Some(this)
}

/// Prefix `p` as a tree at `at` and its `this`, if `this` of a class enclosing `class` through
/// enclosing instances or objects selected on it.
fn prefix_path(l: &mut Lower, p: TypeId, class: SymbolId, at: Span) -> Option<(TNodeId, TNodeId)> {
    match *l.u.types().get(p) {
        Type::This(c) => {
            let c = l.u.class_symbol(c).filter(|&c| c != class)?;
            outer_chain(l, class, c)?;
            let this = l.this(c, at);
            Some((this, this))
        }
        Type::Sym(Some(q), s) if l.u.is_module(s) => {
            let (qualifier, this) = prefix_path(l, q, class, at)?;
            Some((l.select(qualifier, s, at), this))
        }
        _ => None,
    }
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
    if local && !l.outer_accessors.contains_key(&c) { None } else { outer_class(l, c) }
}

/// The accessor `C$$$outer()` of the enclosing instance of class `class`, defined once; `class`
/// then gets its field (dotty's `outerAccName`, `ensureOuterAccessors`).
pub fn outer_accessor(l: &mut Lower, class: SymbolId) -> SymbolId {
    if let Some(&m) = l.outer_accessors.get(&class) {
        return m;
    }
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    let name = outer_accessor_name(l, class);
    let outer = match is_term(l, owner) {
        true => super::local_free::enclosing_class(l, owner),
        false => outer_class(l, class).expect("a class with an enclosing instance"),
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

/// `C$$$outer`, the name of the accessor of the enclosing instance of class `class`, `C` its
/// `separated_name` if in a term.
pub fn outer_accessor_name(l: &Lower, class: SymbolId) -> String {
    match in_term(l, class) {
        true => format!("{}$$$outer", super::local_outer::separated_name(l, class)),
        false => format!("{}$$$outer", l.u.expanded_binary_name(class).replace('.', "$")),
    }
}
