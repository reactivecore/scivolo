// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! Trait parameters in dotty's `Mixin`: a trait's constructor `$init$` takes none, and a class's
//! call of a trait's constructor becomes the arguments that initialize the trait's parameter
//! accessors (`transformConstructor`).

use super::trait_fields::{forwarder, implemented_by};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::{SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

/// A class's call of a trait's constructor: its span and arguments.
pub type TraitCall = (Span, Vec<TNodeId>);

/// Makes trait constructor definition `constr` take no parameters but those standing for the
/// local values it uses, if it takes others.
pub fn strip_params(l: &mut Lower, constr: TNodeId) {
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let lifted: Vec<SymbolId> = l
        .trait_methods
        .get(&ctor)
        .map(|(_, pairs)| pairs.iter().map(|&(_, p)| p).collect())
        .unwrap_or_default();
    let clauses = l.tpd.clause_items(paramss).to_vec();
    let all: Vec<TNodeId> = clauses.iter().flat_map(|&c| l.tpd.items(c)).copied().collect();
    let values = all.iter().filter(|&&p| !matches!(l.tpd.tree(p), Tree::TypeDef { .. })).count();
    if values == lifted.len() {
        return;
    }
    let kept: Vec<TNodeId> =
        all.into_iter().filter(|&p| l.tpd.sym(p).is_some_and(|s| lifted.contains(&s))).collect();
    let kept = l.tpd.list(&kept);
    let paramss = l.tpd.clauses(&[kept]);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs };
    let unit = l.u.class_ref("scala.Unit");
    let binder = l.u.fresh_binder();
    let params = lifted.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p))).collect();
    let info =
        l.u.intern_type(Type::Method { binder, params, result: unit, implicit: false, contextual: false });
    l.u.set_info(ctor, info);
}

/// Parents `parents` of a class with each call of a trait's constructor a type, and the span and
/// arguments of those calls, their clauses joined, by trait.
pub fn split_parents(l: &mut Lower, parents: List) -> Result<(List, FxHashMap<SymbolId, TraitCall>)> {
    let mut args = FxHashMap::default();
    let mut out = Vec::new();
    for p in l.tpd.items(parents).to_vec() {
        let span = l.tpd.span(p);
        let Some((owner, call_args)) = trait_call(l, p) else {
            out.push(p);
            continue;
        };
        if matches!(l.tpd.tree(p), Tree::Block { .. }) {
            return unsupported(span, "a trait parent call with reordered arguments");
        }
        let tpe = l.tpd.tpe(p);
        out.push(l.type_tree(tpe, span));
        args.insert(owner, (span, call_args));
    }
    Ok((l.tpd.list(&out), args))
}

/// What class `class` gets for parameter accessor `getter` of a trait it mixes in, given
/// argument `arg`: a getter returning `arg` if `getter` is the member of `class`, else `arg`
/// to evaluate (dotty's `traitInits`).
pub fn param_init(
    l: &mut Lower,
    class: SymbolId,
    getter: SymbolId,
    arg: Option<TNodeId>,
    span: Span,
) -> Result<TNodeId> {
    let Some(arg) = arg else { return unsupported(span, "a parameterized trait without arguments") };
    if !implemented_by(l, class, getter) {
        return Ok(arg);
    }
    let def = forwarder(l, class, getter, Some(arg));
    let at = l.tpd.span(arg);
    l.tpd.get_mut(def).span = if at.exists() { Span::at(at.start()) } else { at };
    Ok(def)
}

/// The arguments of the calls of traits' constructors among `parents`.
pub fn trait_args(l: &Lower, parents: List) -> Vec<TNodeId> {
    l.tpd.items(parents).iter().filter_map(|&p| trait_call(l, p)).flat_map(|(_, args)| args).collect()
}

/// The trait and the arguments of `parent` if it calls a trait's constructor.
fn trait_call(l: &Lower, parent: TNodeId) -> Option<(SymbolId, Vec<TNodeId>)> {
    let mut fun = parent;
    let mut args = Vec::new();
    loop {
        match *l.tpd.tree(fun) {
            Tree::Block { expr, .. } => fun = expr,
            Tree::Apply { fun: inner, args: clause } => {
                args.splice(0..0, l.tpd.items(clause).iter().copied());
                fun = inner;
            }
            Tree::TypeApply { fun: inner, .. } => fun = inner,
            _ => break,
        }
    }
    let ctor = l.tpd.sym(fun).filter(|&s| l.u.is_constructor(s))?;
    let owner = l.u.symbol(ctor).owner.filter(|&o| l.u.is_trait(o))?;
    Some((owner, args))
}
