//! The untyped trees of derived type class instances: type parameters copied from the deriving
//! class, applied type class and class types.

use super::super::Typer;
use names::nme;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// Makes a new untyped node of `tree` at `span`.
pub fn node(t: &mut Typer<'_>, tree: Tree, span: Span) -> NodeId {
    let node = t.ast.node(tree);
    t.ast.with_span(node, span);
    node
}

/// Makes an untyped node standing for a type tree of `tpe` at `span`.
pub fn type_node(t: &mut Typer<'_>, tpe: TypeId, span: Span) -> NodeId {
    let node = t.splice_type(tpe);
    t.ast.with_span(node, span);
    node
}

/// Makes `tc[args]`.
pub fn tc_applied(t: &mut Typer<'_>, tc: TypeId, args: Vec<NodeId>, span: Span) -> NodeId {
    let tpt = type_node(t, tc, span);
    let args = t.ast.list(&args);
    node(t, Tree::AppliedTypeTree { tpt, args }, span)
}

/// Makes `class[args]`, or `class` without arguments.
pub fn class_applied(t: &mut Typer<'_>, class: TypeId, args: Vec<NodeId>, span: Span) -> NodeId {
    if args.is_empty() {
        return type_node(t, class, span);
    }
    tc_applied(t, class, args, span)
}

/// Makes a reference to type parameter `p` by its name.
pub fn param_ident(t: &mut Typer<'_>, p: SymbolId, span: Span) -> NodeId {
    let name = t.u.symbol(p).name.clone();
    let name = TName { name: t.name(&name), is_type: true };
    node(t, Tree::Ident { name }, span)
}

/// Makes the definition of a type parameter like class type parameter `p`.
pub fn param_def_of(t: &mut Typer<'_>, p: SymbolId, span: Span) -> Result<NodeId, &'static str> {
    let name = t.u.symbol(p).name.clone();
    let info = t.u.info(p);
    param_def(t, &name, info, span)
}

/// Makes the definition of type parameter `name` with info `info`: bounds, or a type lambda
/// over bounds for a higher-kinded one; refused for bounds that refer to other type parameters,
/// as in `class C[A, B <: A] derives Eq`.
pub fn param_def(t: &mut Typer<'_>, name: &str, info: TypeId, span: Span) -> Result<NodeId, &'static str> {
    let mut dependent = false;
    t.u.map_type(info, &mut |u, p| {
        dependent |= match *u.ty(p) {
            Type::Param(..) | Type::Local(..) => true,
            Type::Sym(_, s) => u.symbol(s).kind == Kind::TypeParam,
            _ => false,
        };
        None
    });
    if dependent {
        return Err("derives clause of a class whose type parameter bounds are dependent");
    }

    let inner = t.u.bounds(info).map_or(info, |(_, hi)| hi);
    let lambda = t.u.lambda(inner).map(|l| (l.params.to_vec(), l.result));
    let rhs = match lambda {
        Some((params, result)) => {
            let mut tparams = Vec::new();
            for (i, &(n, b)) in params.iter().enumerate() {
                let n = t.u.name(n);
                let n = if n == nme::WILDCARD { format!("_${}", i + 1) } else { n.to_owned() };
                tparams.push(param_def(t, &n, b, span)?);
            }
            let result = match *t.u.ty(result) {
                Type::Bounds(..) => result,
                _ => t.u.intern_type(Type::Bounds(tasty::NOTHING, result)),
            };
            let body = type_node(t, result, span);
            let tparams = t.ast.list(&tparams);
            node(t, Tree::LambdaTypeTree { tparams, body }, span)
        }
        None => type_node(t, info, span),
    };

    let name = TName { name: t.name(name), is_type: true };
    let mods = t.ast.mods(Modifiers::with_flags(f::TYPE_PARAM));
    Ok(node(t, Tree::TypeDef { name, rhs, mods }, span))
}
