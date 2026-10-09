//! Lambdas and local methods (dotty's `LambdaLift`): each `{ def $anonfun(ps) = body; closure }`
//! and each `def f(ps) = body` in a block of a class's methods, field initializers or statements
//! becomes a private method of the class taking the captured values first; closures and calls
//! pass them.

use super::captures::dependencies;
use super::lifted::{Method, lift, methods};
use super::walk::children;
use crate::symbols::{has, is_class, is_module_class, is_static};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, LAZY};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// A function to lift: its definition, the block it is in, its closure if a lambda, the function
/// it is in, the values it captures, whether it needs `this`, whether it is in the constructor
/// (a field initializer or statement).
pub struct Function {
    pub def: TNodeId,
    pub block: TNodeId,
    pub closure: Option<TNodeId>,
    pub sym: SymbolId,
    pub parent: Option<usize>,
    pub captured: Vec<SymbolId>,
    pub this: bool,
    pub ctor: bool,
}

/// The functions of a class to lift, their methods, the functions in the order their proxies are
/// named and the number of those capturing directly.
struct Lifting {
    class: SymbolId,
    template: TNodeId,
    stats: Vec<TNodeId>,
    functions: Vec<Function>,
    methods: Vec<Method>,
    order: Vec<usize>,
    direct: usize,
}

/// Lifts the lambdas and local methods of class definitions `classes` into methods of their
/// classes; the proxies are named first, those of `$adapted` methods last.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    let mut liftings = Vec::new();
    for &td in classes {
        liftings.extend(prepare(l, td)?);
    }
    let direct = liftings.iter().flat_map(|c| c.order[..c.direct].iter().map(move |&i| &c.methods[i]));
    let propagated = liftings.iter().flat_map(|c| c.order[c.direct..].iter().map(move |&i| &c.methods[i]));
    for method in direct.chain(propagated) {
        name_proxies(l, &method.proxies);
    }
    let mut deferred = Vec::new();
    for c in liftings {
        let (lifted, adapted) = lift(l, c.class, c.template, &c.functions, &c.methods)?;
        deferred.extend(adapted);
        let stats: Vec<TNodeId> = c.stats.into_iter().chain(lifted).collect();
        let body = l.tpd.list(&stats);
        let Tree::Template { constr, parents, self_, .. } = *l.tpd.tree(c.template) else {
            unreachable!("a template")
        };
        l.tpd.get_mut(c.template).tree = Tree::Template { constr, parents, self_, body };
    }
    name_proxies(l, &deferred);
    Ok(())
}

/// Names proxies `proxies`, each `x$N` for the value `x` it stands for.
pub fn name_proxies(l: &mut Lower, proxies: &[SymbolId]) {
    for &p in proxies {
        let base = l.u.symbol(p).name.clone();
        let name = fresh(l, &base);
        l.u.rename(p, &name);
    }
}

/// `base$N`, `N` counting the names made from `base` in the unit (dotty's `freshened`).
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let name = l.fresh.next(l.names, &format!("{base}$"));
    l.names.text(name).to_owned()
}

/// The indices of `functions`, each after the functions in it.
pub fn post_order(functions: &[Function]) -> Vec<usize> {
    fn visit(functions: &[Function], i: Option<usize>, out: &mut Vec<usize>) {
        for j in (0..functions.len()).filter(|&j| functions[j].parent == i) {
            visit(functions, Some(j), out);
            out.push(j);
        }
    }
    let mut out = Vec::new();
    visit(functions, None, &mut out);
    out
}

/// The functions of class definition `td` with their dependencies and methods, if any.
fn prepare(l: &mut Lower, td: TNodeId) -> Result<Option<Lifting>> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return Ok(None) };
    let stats = l.tpd.items(body).to_vec();
    let (mut functions, mut roots) = (Vec::new(), Vec::new());
    for &stat in &stats {
        let method = matches!((l.tpd.tree(stat), l.tpd.sym(stat)),
            (Tree::DefDef { .. }, Some(s)) if !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY));
        let first = functions.len();
        let root = match *l.tpd.tree(stat) {
            Tree::DefDef { rhs: Some(rhs), .. } => rhs,
            Tree::DefDef { .. } | Tree::TypeDef { .. } => continue,
            _ => stat,
        };
        roots.push(root);
        collect(l, root, None, &mut functions);
        if !method {
            for f in &mut functions[first..] {
                f.ctor = true;
            }
        }
    }
    if functions.is_empty() {
        return Ok(None);
    }
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    if is_class(l.u, owner) && !is_static(l.u, class) {
        return unsupported(l.tpd.span(functions[0].def), "lambdas and local methods in nested classes");
    }
    let in_class =
        is_static(l.u, class) && (is_module_class(l.u, class) || l.u.symbol(owner).kind != Kind::Package);
    for f in &mut functions {
        f.this = (f.closure.is_none() || f.ctor) && in_class;
    }
    let (order, direct) = dependencies(l, &mut functions, class, &roots)?;
    let methods = methods(l, class, &functions)?;
    Ok(Some(Lifting { class, template, stats, functions, methods, order, direct }))
}

/// The lambdas and local methods under `t` in pre-order, `parent` the one they are in.
fn collect(l: &Lower, t: TNodeId, parent: Option<usize>, out: &mut Vec<Function>) {
    if let Some((def, closure, sym)) = lambda(l, t) {
        out.push(Function {
            def,
            block: t,
            closure: Some(closure),
            sym,
            parent,
            captured: Vec::new(),
            this: false,
            ctor: false,
        });
        let index = out.len() - 1;
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(def) {
            collect(l, rhs, Some(index), out);
        }
        return;
    }
    match *l.tpd.tree(t) {
        Tree::TypeDef { .. } => {}
        Tree::Block { stats, expr } => {
            for &s in l.tpd.items(stats) {
                match (l.tpd.tree(s), l.tpd.sym(s)) {
                    (Tree::DefDef { rhs: Some(rhs), .. }, Some(sym)) => {
                        let rhs = *rhs;
                        out.push(Function {
                            def: s,
                            block: t,
                            closure: None,
                            sym,
                            parent,
                            captured: Vec::new(),
                            this: false,
                            ctor: false,
                        });
                        let index = out.len() - 1;
                        collect(l, rhs, Some(index), out);
                    }
                    _ => collect(l, s, parent, out),
                }
            }
            collect(l, expr, parent, out);
        }
        _ => {
            for c in children(l.tpd, t) {
                collect(l, c, parent, out);
            }
        }
    }
}

/// The `$anonfun` definition, closure and method symbol of lambda block `t`.
fn lambda(l: &Lower, t: TNodeId) -> Option<(TNodeId, TNodeId, SymbolId)> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[def] = l.tpd.items(stats) else { return None };
    let Tree::Closure { meth, .. } = *l.tpd.tree(expr) else { return None };
    let sym = l.tpd.sym(def)?;
    (matches!(l.tpd.tree(def), Tree::DefDef { .. }) && l.tpd.sym(meth) == Some(sym))
        .then_some((def, expr, sym))
}
