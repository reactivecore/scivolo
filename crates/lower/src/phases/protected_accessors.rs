// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//   compiler/src/dotty/tools/dotc/transform/ProtectedAccessors.scala
//! dotty's `ProtectedAccessors`, with the parts of `AccessProxies` it uses: a protected member
//! used from a class outside its package that does not derive from its owner is reached through
//! `protected$m`, a forwarder of the closest enclosing class that does.

use super::local_free::within;
use super::mixin::{has_body, is_jdk, java_named};
use super::walk;
use crate::definitions::symbol;
use crate::symbols::{is_java_static, is_static_module};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{OVERRIDE, PROTECTED, PROTECTEDqualified, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// An accessor the phase made: its symbol, the member it accesses, whether it sets it.
struct Accessor {
    sym: SymbolId,
    accessed: SymbolId,
    setter: bool,
}

/// Whether a reference to `sym` from class `class` needs an accessor unless `class` derives from
/// the owner of `sym` (dotty's `needsAccessorIfNotInSubclass`).
pub fn needs_accessor_if_not_in_subclass(l: &Lower, sym: SymbolId, class: SymbolId) -> bool {
    let s = l.u.symbol(sym);
    matches!(s.kind, Kind::Def | Kind::Val)
        && (s.flags.has(PROTECTED) || s.flags.has(PROTECTEDqualified))
        && !l.u.is_constructor(sym)
        && !l.u.is_annotated(sym, "publicInBinary")
        && s.owner.is_some_and(|o| !l.u.is_trait(o))
        && !is_java_static(l.u, sym)
        && !within(l, class, package_of(l, sym))
}

/// Whether `sym` referred to on `this` at `span` from class `class` needs a super accessor in
/// the trait hosting its accessor (`SuperAccessors.transformSelect`); refused for a member of a
/// JDK class but `Object` in a trait deriving from its owner, which dotty rejects.
pub fn super_accessor_needed(l: &mut Lower, sym: SymbolId, class: SymbolId, span: Span) -> Result<bool> {
    if !needs_accessor_if_not_in_subclass(l, sym, class)
        || !host_for_accessor_of(l, sym, class).is_some_and(|h| l.u.is_trait(h))
    {
        return Ok(false);
    }
    if !derives_from_owner(l, class, sym) {
        return Ok(true);
    }
    let owner = l.u.symbol(sym).owner.expect("a member's owner");
    match is_jdk(l, owner) && owner != l.defs.object {
        true => unsupported(span, "protected Java members on `this` in concrete trait methods"),
        false => Ok(false),
    }
}

fn derives_from_owner(l: &mut Lower, class: SymbolId, sym: SymbolId) -> bool {
    let owner = l.u.symbol(sym).owner.expect("a member's owner");
    l.u.derives_from(class, owner)
}

/// The package `s` is in.
fn package_of(l: &Lower, mut s: SymbolId) -> SymbolId {
    while l.u.symbol(s).kind != Kind::Package {
        s = l.u.symbol(s).owner.expect("an owner up to a package");
    }
    s
}

/// The closest class enclosing `from` that derives from the owner of `sym` or whose object owns
/// it (dotty's `hostForAccessorOf`).
fn host_for_accessor_of(l: &mut Lower, sym: SymbolId, from: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(sym).owner?;
    let companion = l.u.is_module_class(owner).then(|| l.u.companion_class(owner)).flatten();
    let mut c = from;
    loop {
        if l.u.is_class(c) && (l.u.derives_from(c, owner) || Some(c) == companion) {
            return Some(c);
        }
        c = l.u.symbol(c).owner?;
    }
}

/// Replaces the references of the unit's `classes` needing accessors by references to them, and
/// appends the accessors to the classes hosting them.
pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) -> Result<()> {
    let mut made = Vec::new();
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        walk::try_walk_class_up(l, td, &mut |l, t, _| rewrite(l, t, class, &mut made))?;
    }
    if made.is_empty() {
        return Ok(());
    }
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        let hosted: Vec<&Accessor> = made.iter().filter(|a| l.u.symbol(a.sym).owner == Some(class)).collect();
        if hosted.is_empty() {
            continue;
        }
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        let end = Span::at(l.tpd.span(template).end());
        let defs: Vec<TNodeId> = hosted.into_iter().map(|a| accessor_def(l, root, a, end)).collect();
        let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
            unreachable!("a class definition's template")
        };
        let mut stats = l.tpd.items(body).to_vec();
        stats.extend(defs);
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    }
    Ok(())
}

/// Tree `t` of class `class` referring to an accessor of `made` where it needs one.
fn rewrite(l: &mut Lower, t: TNodeId, class: SymbolId, made: &mut Vec<Accessor>) -> Result<()> {
    let qualifier = match *l.tpd.tree(t) {
        Tree::Select { qualifier, .. } if !matches!(l.tpd.tree(qualifier), Tree::Super { .. }) => {
            Some(qualifier)
        }
        Tree::Ident { .. } => None,
        Tree::Assign { lhs, rhs } => {
            use_setter(l, t, lhs, rhs, made);
            return Ok(());
        }
        _ => return Ok(()),
    };
    let Some(sym) = l.tpd.sym(t) else { return Ok(()) };
    if !needs_accessor_if_not_in_subclass(l, sym, class) || derives_from_owner(l, class, sym) {
        return Ok(());
    }
    let span = l.tpd.span(t);
    let Some(host) = host_for_accessor_of(l, sym, class) else {
        return unsupported(span, "protected members without a class hosting their accessor");
    };
    let this = l.u.this_type(host);
    let info = l.u.member_info(this, sym);
    let info = java_named(l, sym, info);
    if matches!(l.u.types().get(info), Type::Poly { .. }) {
        return unsupported(span, "protected accessors of polymorphic methods");
    }
    let name = format!("protected${}", l.u.symbol(sym).name);
    let acc = accessor(l, host, &name, info, (sym, false), made);
    let new = rewired(l, qualifier, acc, span);
    l.copy_node(t, new);
    Ok(())
}

/// Makes assignment `t` of `rhs` to `lhs` a call of the setter accessor if `lhs` refers to a
/// getter accessor (`AccessProxies.useSetter`).
fn use_setter(l: &mut Lower, t: TNodeId, lhs: TNodeId, rhs: TNodeId, made: &mut Vec<Accessor>) {
    let Some(getter) = l.tpd.sym(lhs) else { return };
    let Some(accessed) = made.iter().find(|a| a.sym == getter && !a.setter).map(|a| a.accessed) else {
        return;
    };
    let host = l.u.symbol(getter).owner.expect("an accessor's class");
    let name = format!("{}_=", l.u.symbol(getter).name);
    let unit = l.u.class_ref("scala.Unit");
    let getter_info = l.u.info(getter);
    let value = l.u.result_type(getter_info);
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".to_owned(), value)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: unit, implicit: false, contextual: false });
    let acc = accessor(l, host, &name, info, (accessed, true), made);
    let qualifier = match *l.tpd.tree(lhs) {
        Tree::Select { qualifier, .. } => Some(qualifier),
        _ => None,
    };
    let fun = rewired(l, qualifier, acc, l.tpd.span(lhs));
    let call = l.apply(fun, &[rhs], unit, l.tpd.span(t));
    l.copy_node(t, call);
}

/// The getter or setter accessor of `accessed` named `name` in `host` of type `info`: the one in
/// `made`, else a new one entered there, overriding a concrete one of a base class
/// (`AccessProxies.useAccessor`).
fn accessor(
    l: &mut Lower,
    host: SymbolId,
    name: &str,
    info: TypeId,
    (accessed, setter): (SymbolId, bool),
    made: &mut Vec<Accessor>,
) -> SymbolId {
    let found = made
        .iter()
        .find(|a| a.accessed == accessed && a.setter == setter && l.u.symbol(a.sym).owner == Some(host));
    if let Some(a) = found {
        return a.sym;
    }
    let flags = Flags::default().with(SYNTHETIC);
    let sym = l.u.define(symbol(name, Kind::Def, host, flags));
    l.u.set_info(sym, info);
    let site = l.u.this_type(host);
    let bases = l.u.linearization(host);
    let inherited: Vec<SymbolId> =
        bases.into_iter().skip(1).flat_map(|b| l.u.decl(b, name).unwrap_or_default()).collect();
    if inherited.into_iter().any(|m| has_body(l, m) && l.u.matches(site, m, sym)) {
        l.u.set_flags(sym, flags.with(OVERRIDE));
    }
    made.push(Accessor { sym, accessed, setter });
    sym
}

/// A reference to `acc` at `span`: on `qualifier` if it derives from the accessor's class, else
/// on that class's `this` (`AccessProxies.rewire`).
fn rewired(l: &mut Lower, qualifier: Option<TNodeId>, acc: SymbolId, span: Span) -> TNodeId {
    let host = l.u.symbol(acc).owner.expect("an accessor's class");
    let on_qualifier = qualifier.filter(|&q| {
        let tpe = l.u.widen(l.tpd.tpe(q));
        l.u.class_symbol(tpe).is_some_and(|c| l.u.derives_from(c, host))
    });
    match on_qualifier {
        Some(q) => l.select(q, acc, span),
        None => reference(l, host, acc, span),
    }
}

/// `sym` of class `class` from inside it: by name in a static object, else on its `this`.
fn reference(l: &mut Lower, class: SymbolId, sym: SymbolId, span: Span) -> TNodeId {
    if is_static_module(l.u, class) {
        return l.ident(sym, span);
    }
    let this = l.this(class, span);
    l.select(this, sym, span)
}

/// The definition of accessor `a`, spanning the name in the definition of the member it accesses
/// if the unit `root` has it, else at `end` (`AccessProxies.accessorDef`).
fn accessor_def(l: &mut Lower, root: TNodeId, a: &Accessor, end: Span) -> TNodeId {
    let defined = walk::subtrees(l.tpd, root).into_iter().find(|&d| {
        l.tpd.sym(d) == Some(a.accessed) && matches!(l.tpd.tree(d), Tree::DefDef { .. } | Tree::ValDef { .. })
    });
    let span = defined.map_or(end, |d| {
        let point = l.tpd.span(d).point();
        Span::new(point, point + l.u.symbol(a.accessed).name.len() as u32, point)
    });
    let host = l.u.symbol(a.sym).owner.expect("an accessor's class");
    let (accessed, setter) = (a.accessed, a.setter);
    let info = l.u.info(accessed);
    let applied = matches!(l.u.types().get(info), Type::Method { .. });
    l.forwarding_def(a.sym, span, |l, args, result| {
        let access = reference(l, host, accessed, span);
        match () {
            _ if setter => l.tpd.node(Tree::Assign { lhs: access, rhs: args[0] }, span, Some(result), None),
            _ if applied => l.apply(access, args, result, span),
            _ => access,
        }
    })
}
