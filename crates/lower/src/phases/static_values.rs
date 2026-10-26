// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MoveStatics.scala
//! `@static` values of an object as Scala Native emulates static fields: fields of the object
//! without initializers, set by a `<clinit>` of the companion class, their setters static methods
//! of the companion (dotty's `MoveStatics` for values).

use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{LAZY, MUTABLE, STATIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes the `@static` values among `stats` of object class `module` with template body `body`
/// fields without initializers; the setters of the variables among them, with their bodies, and
/// a `<clinit>` of companion class `class` storing their initial values, to move there, at the
/// start of the class's first statement `first` if before them.
pub fn transform(
    l: &mut Lower,
    (module, class): (SymbolId, SymbolId),
    (stats, body): (&[TNodeId], &[TNodeId]),
    first: Option<Span>,
) -> Result<Vec<TNodeId>> {
    let mut inits = Vec::new();
    for &stat in stats {
        let Tree::ValDef { name, tpt, rhs } = *l.tpd.tree(stat) else { continue };
        let Some(s) = l.tpd.sym(stat) else { continue };
        if has(l.u, s, LAZY) || l.u.is_module(s) {
            return unsupported(l.tpd.span(stat), "@static lazy values");
        }
        let Some(rhs) = rhs else { return unsupported(l.tpd.span(stat), "@static values without a value") };
        super::static_members::check_needs_no_this(l, rhs)?;
        let flags = l.u.symbol(s).flags.with(STATIC);
        l.u.set_flags(s, flags);
        l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
        inits.push((s, rhs));
    }
    if inits.is_empty() {
        return Ok(Vec::new());
    }
    let source = l.u.source_module(module).expect("an object's value");
    let mut moved = Vec::new();
    let vars: Vec<SymbolId> = inits.iter().map(|&(s, _)| s).filter(|&s| has(l.u, s, MUTABLE)).collect();
    for var in vars {
        let name = format!("{}_=", l.u.symbol(var).name);
        let setter = l.u.decl(module, &name).unwrap_or_default().into_iter().next();
        let def = setter.and_then(|s| body.iter().copied().find(|&d| l.tpd.sym(d) == Some(s)));
        let Some(def) = def else { return unsupported(Span::NONE, "@static variables without a setter") };
        set_setter_body(l, def, source, var);
        moved.push(def);
    }
    let at = l.tpd.span(inits[0].1);
    let at = match first.filter(|f| f.exists() && f.start() < at.start()) {
        Some(f) => Span::at(f.start()),
        None => at,
    };
    moved.push(clinit(l, class, source, &inits, at));
    Ok(moved)
}

/// Makes the body of setter definition `def` the store of its parameter into static field `var`
/// of object `source`.
fn set_setter_body(l: &mut Lower, def: TNodeId, source: SymbolId, var: SymbolId) {
    let Tree::DefDef { name, paramss, tpt, .. } = *l.tpd.tree(def) else { unreachable!("a setter") };
    let span = l.tpd.span(def);
    let at = Span::at(span.start());
    let param = l.tpd.clause_items(paramss).first().and_then(|&c| l.tpd.items(c).first().copied());
    let param = param.and_then(|p| l.tpd.sym(p)).expect("a setter's parameter");
    let value = l.ident(param, at);
    let assign = store(l, source, var, value, at);
    l.tpd.get_mut(def).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(assign) };
}

/// `source.var = value` at `span`.
fn store(l: &mut Lower, source: SymbolId, var: SymbolId, value: TNodeId, span: Span) -> TNodeId {
    let module = l.ident(source, span);
    let lhs = l.select(module, var, span);
    let info = l.u.info(var);
    l.tpd.get_mut(lhs).tpe = Some(l.u.widen(info));
    let unit = l.u.class_ref("scala.Unit");
    l.tpd.node(Tree::Assign { lhs, rhs: value }, span, Some(unit), None)
}

/// `static def <clinit>(): Unit` of class `class` storing each initial value of `inits` into its
/// field of object `source`, at `span`.
fn clinit(
    l: &mut Lower,
    class: SymbolId,
    source: SymbolId,
    inits: &[(SymbolId, TNodeId)],
    span: Span,
) -> TNodeId {
    let flags = Flags::default().with(STATIC);
    let sym = l.u.define(symbol("<clinit>", Kind::Def, class, flags));
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result: unit,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(sym, info);
    let stores: Vec<TNodeId> = inits
        .iter()
        .map(|&(var, rhs)| {
            let span = l.tpd.span(rhs);
            store(l, source, var, rhs, span)
        })
        .collect();
    let done = l.unit_literal(span);
    let body = l.block(&stores, done, span);
    let tpt = l.type_tree(unit, span);
    l.build().def_def(sym, &[&[]], tpt, Some(body), span)
}
