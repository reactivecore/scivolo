// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/CompleteJavaEnums.scala
//   compiler/src/dotty/tools/dotc/transform/MoveStatics.scala
//! On the JVM, the static forwarders of a Java enum class to the enum values of its companion
//! object (dotty's `addedEnumForwarders`, `enumForwarderInitializers`): a static variable per
//! value, set in the class's `<clinit>` and again by the object's constructor.

use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ENUM, MUTABLE, PRIVATE, STATIC};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Adds the forwarders to enum class definition `td`, one of `classes`.
pub fn add(l: &mut Lower, td: TNodeId, classes: &[TNodeId]) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Some(module) = l.u.companion_module(class) else { return Ok(()) };
    let Some(module_class) = l.u.module_class(module) else { return Ok(()) };
    let decls = l.u.decls(module_class).unwrap_or_default();
    let values: Vec<SymbolId> =
        decls.into_iter().filter(|&v| l.u.symbol(v).kind == Kind::Val && has(l.u, v, ENUM)).collect();
    if values.is_empty() {
        return Ok(());
    }
    let Some(&module_td) = classes.iter().find(|&&c| l.tpd.sym(c) == Some(module_class)) else {
        return unsupported(l.tpd.span(td), "a Java enum whose object is in another unit");
    };
    let span = l.tpd.span(td);
    let (mut defs, mut stores, mut resets) = (Vec::new(), Vec::new(), Vec::new());
    for v in values {
        let name = l.u.symbol(v).name.clone();
        let flags = Flags::default().with(STATIC).with(MUTABLE).with(ENUM);
        let f = l.u.define(symbol(&name, Kind::Val, class, flags));
        let info = l.u.info(v);
        let tpe = l.u.widen(info);
        l.u.set_info(f, tpe);
        let tpt = l.type_tree(tpe, Span::NONE);
        defs.push(l.build().val_def(f, tpt, None, Span::NONE));
        let qualifier = l.ident(module, Span::NONE);
        let value = l.select(qualifier, v, Span::NONE);
        stores.push(assign(l, f, value));
        let value = l.ident(v, Span::NONE);
        resets.push(assign(l, f, value));
    }
    defs.push(clinit(l, class, &stores, span));
    super::outer_traits::append(l, td, &defs);
    super::outer_traits::append(l, module_td, &resets);
    Ok(())
}

/// `f = value`.
fn assign(l: &mut Lower, f: SymbolId, value: TNodeId) -> TNodeId {
    let lhs = l.ident(f, Span::NONE);
    let unit = l.u.class_ref(tasty::defn::UNIT);
    l.tpd.node(Tree::Assign { lhs, rhs: value }, Span::NONE, Some(unit), None)
}

/// `private static def <clinit>(): Unit` of `class` running `stores`.
fn clinit(l: &mut Lower, class: SymbolId, stores: &[TNodeId], span: Span) -> TNodeId {
    let flags = Flags::default().with(STATIC).with(PRIVATE);
    let sym = l.u.define(symbol("<clinit>", Kind::Def, class, flags));
    let unit = l.u.class_ref(tasty::defn::UNIT);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_method(binder, &[], unit, false, false);
    l.u.set_info(sym, info);
    let done = l.unit_literal(span);
    let body = l.block(stores, done, span);
    let tpt = l.type_tree(unit, span);
    l.build().def_def(sym, &[&[]], tpt, Some(body), span)
}
