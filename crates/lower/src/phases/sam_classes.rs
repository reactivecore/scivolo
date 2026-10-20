// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/transform/ExpandSAMs.scala
//! Closures of single abstract method types that are not functional interfaces as anonymous
//! classes whose method calls the closure's (dotty's `ExpandSAMs.transformBlock`, `tpd.AnonClass`).

use super::anon_class::{class_symbol, def, instance, param, repeated_elem};
use super::sam::abstract_methods;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{FINAL, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Makes lambda block `block`, of method definition `fn_def` of `anon`, implementing class type
/// `sam`, `{ fn_def; { final class $anon extends sam { final def m(ps) = anon(ps) }; new $anon() } }`.
pub fn to_anon_class(
    l: &mut Lower,
    block: TNodeId,
    fn_def: TNodeId,
    anon: SymbolId,
    sam: TypeId,
) -> Result<()> {
    let span = coord(l, fn_def, anon);
    let class = l.u.class_symbol(sam).expect("a single abstract method class");
    let methods = abstract_methods(l, class);
    let Some(&method) = methods.first() else {
        return unsupported(span, "single abstract method classes without an abstract method");
    };
    if methods.iter().any(|&m| l.u.symbol(m).name != l.u.symbol(method).name) {
        return unsupported(span, "single abstract method types whose expansion needs bridges");
    }
    let mut parents = vec![sam];
    if l.u.is_trait(class) {
        let (object, any) = (l.defs.object, l.u.builtins().any);
        let bases = l.u.linearization(class);
        let sup = bases[1..].iter().copied().find(|&b| !l.u.is_trait(b)).filter(|&b| b != any);
        let sup = sup.unwrap_or(object);
        let head = l.u.base_type(sam, sup).unwrap_or_else(|| type_ref(l.u, sup));
        parents.insert(0, head);
    }
    let first = l.u.class_symbol(parents[0]).expect("a parent class");
    let Some(ctor) = no_arg_constructor(l, first) else {
        return unsupported(span, "single abstract method classes without a constructor of no arguments");
    };
    let owner = l.u.symbol(anon).owner.expect("a closure's owner");
    let cls = class_symbol(l, owner, parents);
    let forwarder = forwarder(l, cls, method, anon, span)?;
    let new = instance(l, cls, ctor, &[forwarder], span);
    let Tree::Block { stats, .. } = *l.tpd.tree(block) else { unreachable!("a lambda's block") };
    l.tpd.get_mut(block).tree = Tree::Block { stats, expr: new };
    Ok(())
}

/// The position of closure method `anon` defined by `fn_def`, as dotty's symbol of it has: the
/// point of the definition, its end if the method takes no parameters.
fn coord(l: &mut Lower, fn_def: TNodeId, anon: SymbolId) -> Span {
    let span = l.tpd.span(fn_def);
    let info = l.u.info(anon);
    match l.u.types().get(info) {
        Type::Method { params, .. } if params.is_empty() => Span::at(span.end()),
        _ => Span::at(span.point()),
    }
}

/// The final method of class `cls` named as `method` calling `anon` with its parameters, at `span`.
fn forwarder(l: &mut Lower, cls: SymbolId, method: SymbolId, anon: SymbolId, span: Span) -> Result<TNodeId> {
    let name = l.u.symbol(method).name.clone();
    let fwd = l.u.define(symbol(&name, Kind::Def, cls, Flags::default().with(SYNTHETIC).with(FINAL)));
    let info = l.u.info(anon);
    l.u.set_info(fwd, info);
    let Type::Method { params, result, .. } = l.u.types().get(info).clone() else {
        return unsupported(span, "polymorphic single abstract methods");
    };
    let params: Vec<SymbolId> = params.iter().map(|(n, t)| param(l, fwd, n, *t)).collect();
    let fun = l.ident(anon, span);
    let args: Vec<TNodeId> = params.iter().map(|&p| l.ident(p, span)).collect();
    let call = l.apply(fun, &args, result, span);
    let d = def(l, fwd, (&[], &params), (result, call), span);
    Ok(l.with_span(d, span))
}

/// The constructor of class `c` taking no arguments, or only repeated ones.
fn no_arg_constructor(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    l.u.decl(c, "<init>").unwrap_or_default().into_iter().find(|&ctor| {
        let mut info = l.u.info(ctor);
        if let Type::Poly { result, .. } = *l.u.types().get(info) {
            info = result;
        }
        let Type::Method { params, result, .. } = l.u.types().get(info).clone() else { return false };
        let no_args = match params[..] {
            [] => true,
            [(_, p)] => repeated_elem(l, p).is_some(),
            _ => false,
        };
        no_args && !matches!(l.u.types().get(result), Type::Method { .. })
    })
}
