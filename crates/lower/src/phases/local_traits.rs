// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The methods and constructors of local traits (dotty's `LambdaLift` for traits): each using
//! local values of the enclosing methods takes them first, as parameters; its calls pass them.

use super::dependencies::Dependencies;
use super::explicit_outer::terms;
use super::leading_params::add_params;
use super::lift_names::{Pending, Proxy};
use super::lifted::substitute;
use super::lifted_calls::make_call;
use super::local_free::{is_method, is_term, within};
use super::local_proxies::proxy;
use super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::tags::LAZY;
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

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
        let Template { body, .. } = l.tpd.class_template(td).expect("a trait definition");
        let trait_ = l.tpd.sym(td).expect("a trait's symbol");
        for stat in l.tpd.items(body).to_vec() {
            let sym = l.tpd.sym(stat);
            let plain = matches!(l.tpd.tree(stat), Tree::DefDef { .. })
                && is_method(l, sym)
                && sym.is_some_and(|s| !has(l.u, s, LAZY));
            let lazy = sym.is_some_and(|s| has(l.u, s, LAZY));
            if lazy && let Some(t) = captured_ref(l, stat, trait_) {
                return unsupported(l.tpd.span(t), "lazy values of local traits capturing values");
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

/// Gives the constructor of local trait definition `td` leading parameters `params` standing
/// for `values` in its parents and initializing statements, recorded as for its methods.
pub fn init_params(l: &mut Lower, td: TNodeId, values: &[SymbolId], params: &[SymbolId]) {
    let Template { constr, parents, body, .. } = l.tpd.class_template(td).expect("a trait definition");
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let info = l.u.info(ctor);
    add_params(l, constr, ctor, params);
    let stats = l.tpd.items(parents).iter().chain(l.tpd.items(body)).copied().collect::<Vec<_>>();
    for stat in stats {
        let member = match l.tpd.tree(stat) {
            Tree::DefDef { .. } => is_method(l, l.tpd.sym(stat)),
            Tree::TypeDef { .. } => true,
            _ => false,
        };
        if !member {
            super::local_classes::redirect(l, stat, values, params, None);
        }
    }
    l.trait_methods.insert(ctor, (info, values.iter().copied().zip(params.iter().copied()).collect()));
}

/// The arguments of the call in `class` of trait constructor `init` at `at`: the constructor
/// parameters of `class` standing for the local values `init` takes.
pub fn init_args(l: &mut Lower, class: SymbolId, init: SymbolId, at: Span) -> Result<Vec<TNodeId>> {
    let values = l.trait_methods.get(&init).map(|(_, pairs)| pairs.clone()).unwrap_or_default();
    let mut args = Vec::new();
    for (v, _) in values {
        let Some(&p) = l.captured_params.get(&(class, v)) else {
            return unsupported(at, "classes mixing in local traits other than local classes");
        };
        args.push(l.ident(p, at));
    }
    Ok(args)
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
        let target = super::walk::without_type_args(l.tpd, reference);
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
