//! The methods of local traits (dotty's `LambdaLift` for traits): each using local values of the
//! enclosing methods takes them first, as parameters; its calls pass them.

use super::dependencies::Dependencies;
use super::explicit_outer::terms;
use super::lift_names::{Pending, Proxy};
use super::lifted::substitute;
use super::lifted_calls::make_call;
use super::local_classes::add_params;
use super::local_free::{is_method, is_term, within};
use super::local_proxies::proxy;
use super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::tags::LAZY;
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Gives each method of local trait definitions `traits` that uses local values by `deps`
/// leading parameters standing for them, put in `pending` to name, and makes its calls in unit
/// `root` pass the values.
pub fn transform(
    l: &mut Lower,
    root: TNodeId,
    traits: &[TNodeId],
    deps: &Dependencies,
    pending: &mut Pending,
) -> Result<()> {
    let mut defs = Vec::new();
    for &td in traits {
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a trait definition") };
        let Tree::Template { body, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
        let trait_ = l.tpd.sym(td).expect("a trait's symbol");
        for stat in l.tpd.items(body).to_vec() {
            let sym = l.tpd.sym(stat);
            let plain = matches!(l.tpd.tree(stat), Tree::DefDef { .. })
                && is_method(l, sym)
                && sym.is_some_and(|s| !has(l.u, s, LAZY));
            if !plain && let Some(t) = captured_ref(l, stat, trait_) {
                return unsupported(
                    l.tpd.span(t),
                    "members of local traits other than methods capturing values",
                );
            }
            let method = sym.filter(|_| plain);
            defs.extend(method.filter(|&m| !deps.of(m).is_empty()).map(|m| (stat, m)));
        }
    }
    let methods: FxHashMap<SymbolId, &[SymbolId]> = defs.iter().map(|&(_, m)| (m, deps.of(m))).collect();
    pass_values(l, root, &methods)?;
    for (def, m) in defs {
        let values = deps.of(m);
        let params: Vec<SymbolId> =
            values.iter().map(|&v| proxy(l, v, m, Kind::Param, Flags::default())).collect();
        let pairs = values.iter().zip(&params);
        pending.proxies.extend(pairs.map(|(&value, &sym)| Proxy { owner: m, value, sym }));
        let info = l.u.info(m);
        add_params(l, def, m, &params);
        let pairs: Vec<(SymbolId, SymbolId)> = values.iter().copied().zip(params).collect();
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(def) {
            substitute(l, rhs, &pairs.iter().copied().collect());
        }
        l.trait_methods.insert(m, (info, pairs));
    }
    Ok(())
}

/// A reference in `stat` to a local value defined outside trait `trait_`.
fn captured_ref(l: &Lower, stat: TNodeId, trait_: SymbolId) -> Option<TNodeId> {
    terms(l, stat, true).into_iter().find(|&t| {
        let used = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::Ident { .. }));
        used.is_some_and(|v| {
            let owner = l.u.symbol(v).owner;
            matches!(l.u.symbol(v).kind, Kind::Val | Kind::Param)
                && owner.is_some_and(|o| is_term(l, o) && !within(l, o, trait_))
        })
    })
}

/// Makes the calls in `root` of the keys of `methods`, and the references to them outside
/// calls, pass their values first.
fn pass_values(l: &mut Lower, root: TNodeId, methods: &FxHashMap<SymbolId, &[SymbolId]>) -> Result<()> {
    let trees = subtrees(l.tpd, root);
    let funs: FxHashSet<TNodeId> = trees
        .iter()
        .filter_map(|&t| match *l.tpd.tree(t) {
            Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => Some(fun),
            _ => None,
        })
        .collect();
    for t in trees {
        let (reference, args) = match *l.tpd.tree(t) {
            Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
            Tree::TypeApply { fun, .. }
                if !funs.contains(&t) && l.tpd.sym(fun).is_some_and(|m| methods.contains_key(&m)) =>
            {
                return unsupported(l.tpd.span(t), "references to generic methods of local traits");
            }
            Tree::Select { .. } | Tree::Ident { .. } if !funs.contains(&t) => (t, Vec::new()),
            _ => continue,
        };
        let target = match *l.tpd.tree(reference) {
            Tree::TypeApply { fun, .. } => fun,
            _ => reference,
        };
        let called = l
            .tpd
            .sym(target)
            .filter(|_| matches!(l.tpd.tree(target), Tree::Select { .. } | Tree::Ident { .. }));
        let Some((m, &values)) = called.and_then(|m| methods.get(&m).map(|vs| (m, vs))) else { continue };
        let span = l.tpd.span(reference);
        let fun = match reference == t {
            true => {
                let n = l.tpd.get(t);
                let (tree, span, tpe, sym) = (n.tree.clone(), n.span, n.tpe, n.sym);
                l.tpd.node(tree, span, tpe, sym)
            }
            false => reference,
        };
        let mut all: Vec<TNodeId> = values.iter().map(|&v| l.ident(v, span.end_pos())).collect();
        all.extend(args);
        make_call(l, t, m, reference, fun, all);
    }
    Ok(())
}
