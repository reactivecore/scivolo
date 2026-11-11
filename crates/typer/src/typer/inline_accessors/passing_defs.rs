// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The definitions of inline accessors that take the accessed member's receiver as their first
//! parameter (dotty's `MakeInlineablePassing`, `accessorDef` with `passReceiverAsArg`).

use super::super::Typer;
use super::Access;
use super::accessor_defs::accessor_span;
use super::accessor_types::passing_tpt;
use super::passing::Receiver;
use crate::tpd::{TNodeId, Tree as T};
use names::name_kinds;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Type, TypeId, Universe};

/// The name the receiver parameter of a setter has until it is typed, renamed to `x$0` then.
const SETTER_RECEIVER: &str = "x$0$receiver";

/// The names and types of one value parameter clause.
type Params = Vec<(tasty::NameId, TypeId)>;

/// Makes `def name[Ls](x$0: R)(ps): T = x$0.m(ps)` for `access` of member `m` and receivers of
/// type `R` over the inline method's types `Ls`, or `def name[Ls](x$0: R)(x$0: T): Unit =
/// x$0.m = x$0` for a setter; `None`, reported at `span`, for a polymorphic or implicit method.
pub fn passing_def(
    t: &mut Typer<'_>,
    name: &str,
    access: Access,
    receiver: &Receiver,
    span: Span,
) -> Option<NodeId> {
    let (accessed, setter, locals) = (access.member, access.setter, &receiver.locals[..]);
    t.complete(accessed);
    let info = t.u.member_info(receiver.tpe, accessed);
    let info = dealiased(t.u, info);
    let (mut clauses, info) = value_clauses(t, info, setter, span)?;

    let at = accessor_span(t, accessed, span);
    let mut defs = Vec::new();
    if !locals.is_empty() {
        let mut tparams = Vec::new();
        for &l in locals {
            let name = TName { name: t.name(&t.u.symbol(l).name.clone()), is_type: true };
            let bounds = t.u.info(l);
            let rhs = passing_tpt(t, bounds, locals, span)?;
            let mods = t.ast.mods(Modifiers::with_flags(f::TYPE_PARAM));
            tparams.push(t.ast.node(Tree::TypeDef { name, rhs, mods }));
        }
        defs.push(t.ast.list(&tparams));
    }
    let x0 = name_kinds::synthetic_param(0);
    let own = if setter { SETTER_RECEIVER } else { &x0 };
    let recv_tpt = passing_tpt(t, receiver.tpe, locals, span)?;
    let recv = passing_param(t, own, recv_tpt);
    defs.push(t.ast.list(&[recv]));
    let own_name = TName { name: t.name(own), is_type: false };
    let qualifier = t.ast.node(Tree::Ident { name: own_name });
    let member = TName { name: t.name(&t.u.symbol(accessed).name.clone()), is_type: false };
    let mut rhs = t.ast.node(Tree::Select { qualifier, name: member });
    let result = if setter {
        clauses.push(vec![(t.u.intern_name(&x0), info)]);
        crate::defn::type_ref(t.u, t.defn.unit)
    } else {
        info
    };

    for clause in &clauses {
        let mut params = Vec::new();
        let mut args = Vec::new();
        for &(pname, ptype) in clause {
            let pname = t.u.name(pname).to_owned();
            let tpt = passing_tpt(t, ptype, locals, span)?;
            params.push(passing_param(t, &pname, tpt));
            let name = TName { name: t.name(&pname), is_type: false };
            args.push(t.ast.node(Tree::Ident { name }));
        }
        rhs = match setter {
            true => t.ast.node(Tree::Assign { lhs: rhs, rhs: args[0] }),
            false => {
                let args = t.ast.list(&args);
                t.ast.node(Tree::Apply { fun: rhs, args })
            }
        };
        defs.push(t.ast.list(&params));
    }

    let tpt = passing_tpt(t, result, locals, span)?;
    let paramss = t.ast.clauses(&defs);
    let private = t.u.symbol(accessed).flags.has(tasty::tags::PRIVATE);
    let flags = f::SYNTHETIC.or(if private { f::FINAL } else { f::EMPTY });
    let mods = t.ast.mods(Modifiers::with_flags(flags));
    let name = TName { name: t.name(name), is_type: false };
    let def = t.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods });
    t.ast.with_span(def, at);
    Some(def)
}

/// The value parameter clauses of the accessed member's type `info`, none for a setter, and the
/// result type after them, by-name result unwrapped; `None`, reported at `span`, for a
/// polymorphic method or one with implicit parameters.
fn value_clauses(
    t: &mut Typer<'_>,
    mut info: TypeId,
    setter: bool,
    span: Span,
) -> Option<(Vec<Params>, TypeId)> {
    let mut clauses = Vec::new();
    loop {
        match *t.u.ty(info) {
            Type::Method { params, result, implicit: false, .. } if !setter => {
                clauses.push(t.u.params_of(params).to_vec());
                info = result;
            }
            Type::Method { .. } | Type::Poly { .. } => {
                t.unsupported_at(span, "inline accessor of a polymorphic or implicit method");
                return None;
            }
            _ => break,
        }
    }
    if let Some(r) = t.u.by_name(info) {
        info = r;
    }
    Some((clauses, info))
}

/// Names the receiver of typed passing setter definition `def`, and its reference, `x$0`.
pub fn with_setter_receiver(t: &mut Typer<'_>, def: TNodeId) {
    let T::DefDef { paramss, rhs: Some(rhs), .. } = *t.tpd.tree(def) else { return };
    let clauses = t.tpd.clause_items(paramss);
    let params = clauses.iter().flat_map(|&c| t.tpd.items(c));
    let Some(&recv) = params.clone().find(|&&p| matches!(t.tpd.tree(p), T::ValDef { .. })) else {
        return;
    };
    let T::ValDef { name, tpt, rhs: none } = *t.tpd.tree(recv) else { return };
    if t.names.text(name.name) != SETTER_RECEIVER {
        return;
    }
    let x0 = name_kinds::synthetic_param(0);
    let name = TName { name: t.name(&x0), is_type: false };
    t.tpd.get_mut(recv).tree = T::ValDef { name, tpt, rhs: none };
    let sym = t.tpd.sym(recv).expect("a parameter");
    t.u.rename(sym, &x0);
    if let T::Assign { lhs, .. } = *t.tpd.tree(rhs)
        && let T::Select { qualifier, .. } = *t.tpd.tree(lhs)
    {
        t.tpd.get_mut(qualifier).tree = T::Ident { name };
    }
}

fn passing_param(t: &mut Typer<'_>, name: &str, tpt: NodeId) -> NodeId {
    let name = TName { name: t.name(name), is_type: false };
    let mods = t.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
    t.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
}

/// Type `t` with its aliases expanded, part by part.
pub fn dealiased(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, p| {
        let d = u.dealias(p);
        (d != p).then(|| dealiased(u, d))
    })
}
