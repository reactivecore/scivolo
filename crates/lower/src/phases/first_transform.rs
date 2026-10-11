//! The constant folding of dotty's `FirstTransform`: a pure reference to a value of constant
//! type, and a pure block of constant type, becomes the constant.
#![allow(non_upper_case_globals)]

use super::getters::is_const_final;
use super::walk::walk_class_up;
use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use std::ops::ControlFlow;
use syntax::ast::constant::Constant;
use syntax::names::to_utf16;
use tasty::tags::{
    CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LAZY, LONGconst, MUTABLE, TRUEconst,
};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree, Walk};

/// Folds the references to values of constant type and the constant blocks in class definition
/// `td`, inner trees first; `inits` are the unit's objects with initialization code.
pub fn transform(l: &mut Lower, td: TNodeId, inits: &FxHashSet<SymbolId>) {
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        if let Tree::NamedArg { arg, .. } = *l.tpd.tree(t) {
            l.copy_node(t, arg);
        }
        const_to_literal(l, t, inits);
        ControlFlow::Continue(())
    });
}

/// The objects among class definitions `classes` with initialization code: a value with an
/// initializer or a statement (dotty's not `NoInits`).
pub fn objects_with_inits(l: &mut Lower, classes: &[TNodeId]) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Some(class) = l.tpd.sym(td).filter(|&c| l.u.is_module_class(c)) else { continue };
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
        let initializes = |&s: &TNodeId| match *l.tpd.tree(s) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => false,
            Tree::ValDef { rhs, .. } => {
                rhs.is_some() && l.tpd.sym(s).is_some_and(|v| !has(l.u, v, LAZY) && !l.u.is_module(v))
            }
            _ => true,
        };
        if l.tpd.items(body).iter().any(initializes) {
            out.extend(l.u.source_module(class));
        }
    }
    out
}

/// Makes `t` a literal if it is a pure path to a value of constant type, the path through
/// objects without `inits` unless the value is `final`, or a block of constant type with only a
/// constant.
fn const_to_literal(l: &mut Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) {
    if is_constant_op(l, t, inits)
        && let Some(value) = constant(l.u.types().get(l.tpd.tpe(t)))
    {
        l.tpd.get_mut(t).tree = Tree::Literal { value };
        l.tpd.get_mut(t).sym = None;
        return;
    }
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
    let Some(s) = l.tpd.sym(t).filter(|_| is_pure(l, t)) else { return };
    if !is_const_final(l, s) && through(l, t, inits) {
        return;
    }
    let info = l.u.info(s);
    let Some(value) = constant(l.u.types().get(info)) else { return };
    let node = l.tpd.get_mut(t);
    node.tree = Tree::Literal { value };
    node.tpe = Some(info);
    node.sym = None;
}

/// Whether path `t` selects through one of objects `inits`.
fn through(l: &Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(t) else { return false };
    l.tpd.sym(qualifier).is_some_and(|q| inits.contains(&q)) || through(l, qualifier, inits)
}

/// Whether `t` is an operation of a primitive class or `String` on literals and pure paths, not
/// through objects `inits` (dotty's `isPureApply` for a constant type).
fn is_constant_op(l: &Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let operand =
        |a: TNodeId| matches!(l.tpd.tree(a), Tree::Literal { .. }) || is_pure(l, a) && !through(l, a, inits);
    let (fun, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        Tree::Select { .. } => (t, Vec::new()),
        _ => return false,
    };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return false };
    let Some(owner) = l.tpd.sym(fun).and_then(|m| l.u.symbol(m).owner) else { return false };
    let owner = l.u.full_name(owner);
    let known = PRIMITIVES.contains(&owner.as_str()) || owner == "java.lang.String";
    known && operand(qualifier) && args.into_iter().all(operand)
}

const PRIMITIVES: [&str; 8] = [
    "scala.Boolean",
    "scala.Byte",
    "scala.Short",
    "scala.Char",
    "scala.Int",
    "scala.Long",
    "scala.Float",
    "scala.Double",
];

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
pub fn is_pure(l: &Lower, t: TNodeId) -> bool {
    let stable = |s| {
        let sym = l.u.symbol(s);
        l.u.is_module(s)
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
