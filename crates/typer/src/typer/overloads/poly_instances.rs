// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Polymorphic types instantiated with fresh types (dotty's `constrained`, `newTypeParams`), or
//! with the type arguments of the resolution of `f[Int]` in progress (`Typer::mapped_targs`).

use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId, Type, TypeId};

/// Instantiates polymorphic type `info` of `owner`: its type parameters replaced by fresh
/// abstract types within their bounds if `rigid`, else by fresh type variables within them;
/// `info` itself if not polymorphic.
pub fn poly_instance(t: &mut Typer, info: TypeId, owner: SymbolId, rigid: bool) -> TypeId {
    let Type::Poly { binder, params, result } = t.u.types().get(info).clone() else { return info };
    let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
    if let Some(args) = mapped_targs_of(t, owner, names.len()) {
        return t.instantiate_poly(result, binder, &names, &args, Some(owner));
    }

    let mut args = Vec::new();
    for n in &names {
        args.push(if rigid {
            let s = t.syms.define(t.u, n, Kind::TypeParam, owner, syntax::ast::flags::TYPE_PARAM);
            t.u.unenter(s);
            t.u.intern_type(Type::Sym(None, s))
        } else {
            t.constraint.fresh(t.u)
        });
    }
    for (&(_, bounds), &a) in params.iter().zip(&args) {
        let bounds = t.instantiate_poly(bounds, binder, &names, &args, Some(owner));
        if let Type::Sym(_, s) = *t.u.types().get(a) {
            t.u.set_info(s, bounds);
        } else if let Type::Bounds(lo, hi) = *t.u.types().get(bounds) {
            t.constraint.declare(t.u, a, lo, hi);
            t.conforms(lo, a);
            t.conforms(a, hi);
        }
    }
    t.instantiate_poly(result, binder, &names, &args, Some(owner))
}

/// Finds the type arguments `alt` is resolved with, if it is an alternative of the resolution
/// with instantiated type parameters in progress and takes `n` type parameters.
fn mapped_targs_of(t: &Typer, alt: SymbolId, n: usize) -> Option<Vec<TypeId>> {
    let (alts, targs) = t.mapped_targs.as_ref()?;
    (alts.contains(&alt) && targs.len() == n).then(|| targs.clone())
}

/// Instantiates polymorphic type `info` of `alt` if `alt` is resolved with type arguments, else
/// gives `info`.
pub fn mapped_info(t: &mut Typer, info: TypeId, alt: SymbolId) -> TypeId {
    match t.u.types().get(info) {
        Type::Poly { params, .. } if mapped_targs_of(t, alt, params.len()).is_some() => {
            poly_instance(t, info, alt, false)
        }
        _ => info,
    }
}

/// Types `arg` against `pt` with no alternatives resolved with instantiated type parameters.
pub fn typed_unmapped(t: &mut Typer, arg: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
    let mapped = t.mapped_targs.take();
    let typed = t.typed(arg, pt, ctx);
    t.mapped_targs = mapped;
    typed
}
