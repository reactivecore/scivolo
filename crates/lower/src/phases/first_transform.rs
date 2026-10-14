//! The constant folding of dotty's `FirstTransform`: a pure reference to a value of constant
//! type, and a pure block of constant type, becomes the constant.
#![allow(non_upper_case_globals)]

use super::getters::is_const_final;
use super::module_purity::is_pure_value;
use super::walk::walk_class_up;
use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use std::ops::ControlFlow;
use syntax::ast::constant::Constant;
use syntax::names::to_utf16;
use tasty::tags::{
    CHARconst, CLASSconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LAZY, LONGconst, MUTABLE, TRUEconst,
};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Folds the references to values of constant type and the constant blocks in class definition
/// `td`, inner trees first; `inits` are the unit's objects with initialization code.
pub fn transform(l: &mut Lower, td: TNodeId, inits: &FxHashSet<SymbolId>) {
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        if let Tree::NamedArg { arg, .. } = *l.tpd.tree(t) {
            l.copy_node(t, arg);
        }
        super::fold_condition::fold(l, t);
        const_to_literal(l, t, inits);
        ControlFlow::Continue(())
    });
}

/// The objects among class definitions `classes` with initialization code: a value with an
/// initializer, a statement or a parent constructor called with arguments (dotty's not
/// `NoInits`).
pub fn objects_with_inits(l: &mut Lower, classes: &[TNodeId]) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Some(class) = l.tpd.sym(td).filter(|&c| l.u.is_module_class(c)) else { continue };
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, parents, .. } = *l.tpd.tree(rhs) else { continue };
        let calls_with_args = |&p: &TNodeId| match *l.tpd.tree(p) {
            Tree::Apply { args, .. } => !l.tpd.items(args).is_empty(),
            Tree::Block { .. } => true,
            _ => false,
        };
        if l.tpd.items(parents).iter().any(calls_with_args) {
            out.extend(l.u.source_module(class));
            continue;
        }
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
    if let Some(tpe) = class_of_arg(l, t) {
        let node = l.tpd.get_mut(t);
        node.tree = Tree::ClassLiteral { tpe };
        node.sym = None;
        return;
    }
    if super::get_class::fold(l, t, inits) {
        return;
    }
    if is_constant_op(l, t, inits)
        && let Some(lit) = literal(l.u.types().get(l.tpd.tpe(t)))
    {
        l.tpd.get_mut(t).tree = lit;
        l.tpd.get_mut(t).sym = None;
        return;
    }
    if is_constant_block(l, t) {
        let tpe = match *l.u.types().get(l.tpd.tpe(t)) {
            Type::Sym(_, s) if l.u.symbol(s).kind == Kind::Val => l.u.info(s),
            _ => l.tpd.tpe(t),
        };
        if let Some(lit) = literal(l.u.types().get(tpe)) {
            let node = l.tpd.get_mut(t);
            node.tree = lit;
            return;
        }
    }
    let Some(s) = l.tpd.sym(t).filter(|_| is_pure(l, t)) else { return };
    if !is_const_final(l, s) && through(l, t, inits) {
        return;
    }
    let underlying = term_ref_underlying(l, l.tpd.tpe(t));
    let info = if literal(l.u.types().get(underlying)).is_some() { underlying } else { l.u.info(s) };
    let Some(lit) = literal(l.u.types().get(info)) else { return };
    let node = l.tpd.get_mut(t);
    node.tree = lit;
    node.tpe = Some(info);
    node.sym = None;
}

/// `t` with references to terms and by-name types replaced by their underlying types, as seen
/// from their prefixes (dotty's `widenTermRefExpr`).
fn term_ref_underlying(l: &mut Lower, mut t: TypeId) -> TypeId {
    for _ in 0..64 {
        let next = match *l.u.types().get(t) {
            Type::ByName(r) => Some(r),
            Type::TermRef(..) => l.u.widen_once(t),
            Type::Sym(_, s) if !l.u.is_type_symbol(s) => l.u.widen_once(t),
            _ => None,
        };
        let Some(next) = next else { return t };
        t = next;
    }
    t
}

/// `C` if `t` is `classOf[C]`.
fn class_of_arg(l: &Lower, t: TNodeId) -> Option<TypeId> {
    let Tree::TypeApply { fun, args } = *l.tpd.tree(t) else { return None };
    let &[arg] = l.tpd.items(args) else { return None };
    l.tpd.sym(fun).is_some_and(|s| super::adapt::is_class_of(l, s)).then(|| l.tpd.tpe(arg))
}

/// Whether path `t` selects through one of objects `inits`.
pub fn through(l: &Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(t) else { return false };
    l.tpd.sym(qualifier).is_some_and(|q| inits.contains(&q)) || through(l, qualifier, inits)
}

/// Whether `t` is an operation known to be pure on literals and pure paths, not through objects
/// `inits` (dotty's `isPureApply` for a constant type).
fn is_constant_op(l: &Lower, t: TNodeId, inits: &FxHashSet<SymbolId>) -> bool {
    let operand = |mut a: TNodeId| {
        while let Tree::Typed { expr, .. } = *l.tpd.tree(a) {
            a = expr;
        }
        is_literal(l, a) || is_pure(l, a) && !through(l, a, inits)
    };
    let (fun, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        Tree::Select { .. } => (t, Vec::new()),
        _ => return false,
    };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return false };
    let known = l.tpd.sym(fun).is_some_and(|m| super::pure_ops::is_known_pure_op(l, m));
    known && operand(qualifier) && args.into_iter().all(operand)
}

/// Whether `t` is a block of pure definitions ending in a literal (dotty's `isIdempotentExpr`
/// with `statPurity`).
fn is_constant_block(l: &Lower, t: TNodeId) -> bool {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return false };
    let pure_stat = |&s: &TNodeId| match *l.tpd.tree(s) {
        Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => true,
        Tree::ValDef { rhs, .. } => {
            let mutable = l.tpd.sym(s).is_some_and(|v| has(l.u, v, MUTABLE) || has(l.u, v, LAZY));
            !mutable && rhs.is_some_and(|r| is_pure_value(l, r))
        }
        _ => false,
    };
    l.tpd.items(stats).iter().all(pure_stat) && matches!(l.tpd.tree(expr), Tree::Literal { .. })
}

/// Whether parameter `s` is by-name, its type known.
pub fn is_by_name(l: &Lower, s: tasty::SymbolId) -> bool {
    l.u.known_info(s).is_some_and(|t| matches!(l.u.types().get(t), Type::ByName(_)))
}

/// `is_pure` before `ElimByName`: a reference to a by-name parameter evaluates it.
pub fn is_pure_with_by_name(l: &Lower, t: TNodeId) -> bool {
    let mut path = t;
    loop {
        if l.tpd.sym(path).is_some_and(|s| is_by_name(l, s)) {
            return false;
        }
        match *l.tpd.tree(path) {
            Tree::Select { qualifier, .. } => path = qualifier,
            _ => return is_pure(l, t),
        }
    }
}

pub fn is_literal(l: &Lower, t: TNodeId) -> bool {
    matches!(l.tpd.tree(t), Tree::Literal { .. } | Tree::ClassLiteral { .. })
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

/// The literal of constant type `t`, if the syntax has one for it.
pub fn literal(t: &Type) -> Option<Tree> {
    if let Type::Constant(CLASSconst, bits) = *t {
        return Some(Tree::ClassLiteral { tpe: TypeId(bits as u32) });
    }
    Some(Tree::Literal { value: constant(t)? })
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
    let lit = literal(l.u.types().get(info))?;
    Some(l.tpd.node(lit, span, Some(info), None))
}
