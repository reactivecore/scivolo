//! The constant folding of dotty's `FirstTransform`: a pure reference to a constant `final`
//! value, and a pure block of constant type, becomes the constant.
#![allow(non_upper_case_globals)]

use super::getters::is_const_final;
use super::walk::children;
use crate::Lower;
use crate::symbols::{has, is_module};
use syntax::ast::constant::Constant;
use syntax::names::to_utf16;
use tasty::tags::{
    CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LAZY, LONGconst, MUTABLE, TRUEconst,
};
use tasty::{Kind, Type};
use typer::tpd::{TNodeId, Tree};

/// Folds the references to constant `final` values and the constant blocks in class definition
/// `td`, inner trees first.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let mut trees = Vec::new();
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        trees.push(t);
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t));
        }
    }
    for &t in trees.iter().rev() {
        if let Tree::NamedArg { arg, .. } = *l.tpd.tree(t) {
            l.copy_node(t, arg);
        }
        const_to_literal(l, t);
    }
}

/// Makes `t` a literal if it refers to a constant `final` value through a pure path, or is a
/// block of constant type with only a constant.
fn const_to_literal(l: &mut Lower, t: TNodeId) {
    if is_constant_block(l, t) {
        let tpe = match *l.u.types().get(l.tpd.tpe(t)) {
            Type::Sym(_, s) if l.u.symbol(s).kind == Kind::Val => l.u.info(s),
            _ => l.tpd.tpe(t),
        };
        if let Some(value) = constant(l.u.types().get(tpe)) {
            let node = l.tpd.get_mut(t);
            node.tree = Tree::Literal { value };
            return;
        }
    }
    let pure = match *l.tpd.tree(t) {
        Tree::Ident { .. } => true,
        Tree::Select { qualifier, .. } => is_pure(l, qualifier),
        _ => false,
    };
    let Some(s) = l.tpd.sym(t).filter(|_| pure) else { return };
    if !is_const_final(l, s) {
        return;
    }
    let info = l.u.info(s);
    let Some(value) = constant(l.u.types().get(info)) else { return };
    let node = l.tpd.get_mut(t);
    node.tree = Tree::Literal { value };
    node.tpe = Some(info);
    node.sym = None;
}

/// Whether `t` is a block of pure definitions ending in a literal (dotty's `isIdempotentExpr`
/// with `statPurity`).
fn is_constant_block(l: &Lower, t: TNodeId) -> bool {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return false };
    let pure_stat = |&s: &TNodeId| match *l.tpd.tree(s) {
        Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => true,
        Tree::ValDef { rhs, .. } => {
            let mutable = l.tpd.sym(s).is_some_and(|v| has(l.u, v, MUTABLE) || has(l.u, v, LAZY));
            !mutable && rhs.is_some_and(|r| matches!(l.tpd.tree(r), Tree::Literal { .. }) || is_pure(l, r))
        }
        _ => false,
    };
    l.tpd.items(stats).iter().all(pure_stat) && matches!(l.tpd.tree(expr), Tree::Literal { .. })
}

/// Whether evaluating `t` has no effect (dotty's `isPureExpr` for paths): `this`, an object,
/// a parameter or a stable value, through stable selections.
fn is_pure(l: &Lower, t: TNodeId) -> bool {
    let stable = |s| {
        let sym = l.u.symbol(s);
        is_module(l.u, s)
            || sym.kind == Kind::Param
            || (sym.kind == Kind::Val && !has(l.u, s, LAZY) && !has(l.u, s, MUTABLE))
    };
    match *l.tpd.tree(t) {
        Tree::This { .. } => true,
        Tree::Ident { .. } => l.tpd.sym(t).is_some_and(stable),
        Tree::Select { qualifier, .. } => l.tpd.sym(t).is_some_and(stable) && is_pure(l, qualifier),
        _ => false,
    }
}

/// The constant of constant type `t`, if the syntax has one for it.
fn constant(t: &Type) -> Option<Constant> {
    Some(match *t {
        Type::Constant(TRUEconst, _) => Constant::Boolean(true),
        Type::Constant(FALSEconst, _) => Constant::Boolean(false),
        Type::Constant(CHARconst, b) => Constant::Char(b as u16),
        Type::Constant(INTconst, b) => Constant::Int(b as i32),
        Type::Constant(LONGconst, b) => Constant::Long(b),
        Type::Constant(FLOATconst, b) => Constant::Float(f32::from_bits(b as u32)),
        Type::Constant(DOUBLEconst, b) => Constant::Double(f64::from_bits(b as u64)),
        Type::Str(ref s) => Constant::String(to_utf16(s).into()),
        _ => return None,
    })
}

/// The constant value of constant `final` value `s`, as a literal tree at `span`.
pub fn literal_of(l: &mut Lower, s: tasty::SymbolId, span: syntax::ast::Span) -> Option<TNodeId> {
    let info = l.u.info(s);
    let value = constant(l.u.types().get(info))?;
    Some(l.tpd.node(Tree::Literal { value }, span, Some(info), None))
}
