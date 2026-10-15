//! Lambdas and local methods (dotty's `LambdaLift`): each `{ def $anonfun(ps) = body; closure }`
//! and each `def f(ps) = body` in a block of a class's methods, field initializers or statements
//! becomes a private method taking the captured values first, of the enclosing class it is
//! narrowed into, of the class if it needs `this`, else static in the top-level class; closures
//! and calls pass them.

use super::captures::dependencies;
use super::lift_names::{Named, Pending, name_all, name_each};
use super::lifted::{Method, lift, methods};
use super::walk::children;
use crate::symbols::{has, is_static};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, LAZY, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// A function to lift: its definition, the block it is in, its closure if a lambda, the function
/// it is in, the values it captures, whether it needs `this`, the enclosing class it is lifted
/// into instead, whether it is in the constructor (a field initializer or statement).
pub struct Function {
    pub def: TNodeId,
    pub block: TNodeId,
    pub closure: Option<TNodeId>,
    pub sym: SymbolId,
    pub parent: Option<usize>,
    pub captured: Vec<SymbolId>,
    pub this: bool,
    pub into: Option<SymbolId>,
    pub ctor: bool,
}

impl Function {
    /// Whether its method is an instance method.
    pub fn instance(&self) -> bool {
        self.this || self.into.is_some()
    }

    /// Whether this is a lambda's method, kept as a closure or not (dotty's `isAnonymousFunction`).
    pub fn is_anonymous(&self, l: &Lower) -> bool {
        self.closure.is_some() || l.u.symbol(self.sym).name == "$anonfun"
    }
}

/// The functions of a class to lift and their methods.
struct Lifting {
    td: TNodeId,
    class: SymbolId,
    functions: Vec<Function>,
    methods: Vec<Method>,
}

/// Lifts the lambdas and local methods of class definitions `classes` into methods of their
/// classes, naming them with the proxies and local classes of `pending`; the proxies are named
/// first, those of `$adapted` methods last.
pub fn transform(l: &mut Lower, classes: &[TNodeId], mut pending: Pending) -> Result<()> {
    let mut liftings = Vec::new();
    for &td in classes {
        liftings.extend(prepare(l, td, &pending)?);
    }
    let named: Vec<Named> = liftings
        .iter()
        .map(|c| Named { class: c.class, functions: &c.functions, methods: &c.methods })
        .collect();
    name_all(l, &named, &mut pending)?;
    let lifted: Vec<_> = liftings.iter().map(|c| (c.class, &c.functions[..], &c.methods[..])).collect();
    for caller in &pending.callers {
        super::lifted_calls::calls_from_class(l, caller, &lifted)?;
    }
    let mut deferred = Vec::new();
    for c in liftings {
        let (lifted, adapted) = lift(l, (c.td, c.class), &c.functions, &c.methods)?;
        deferred.extend(adapted);
        for def in lifted {
            let owner = l.tpd.sym(def).and_then(|s| l.u.symbol(s).owner);
            let Some(&td) = classes.iter().find(|&&td| l.tpd.sym(td) == owner) else {
                return unsupported(l.tpd.span(def), "lifting into a class of another unit");
            };
            super::outer_traits::append(l, td, &[def]);
        }
    }
    name_each(l, &deferred);
    Ok(())
}

/// The functions of class definition `td` with their dependencies and methods, if any.
fn prepare(l: &mut Lower, td: TNodeId, pending: &Pending) -> Result<Option<Lifting>> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return Ok(None) };
    let stats = l.tpd.items(body).to_vec();
    let (mut functions, mut roots, mut in_static) = (Vec::new(), Vec::new(), Vec::new());
    for &stat in &stats {
        let method = matches!((l.tpd.tree(stat), l.tpd.sym(stat)),
            (Tree::DefDef { .. }, Some(s)) if !has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY));
        let first = functions.len();
        if l.tpd.sym(stat).is_some_and(|s| has(l.u, s, STATIC)) {
            in_static.push(first);
        }
        let root = match *l.tpd.tree(stat) {
            Tree::DefDef { rhs: Some(rhs), .. } => rhs,
            Tree::DefDef { .. } | Tree::TypeDef { .. } => continue,
            _ => stat,
        };
        roots.push(root);
        collect(l, root, None, &mut functions);
        if in_static.last() == Some(&first) {
            in_static.extend(first + 1..functions.len());
        }
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
    let in_class =
        is_static(l.u, class) && (l.u.is_module_class(class) || l.u.symbol(owner).kind != Kind::Package);
    for f in &mut functions {
        f.this = (!f.is_anonymous(l) || f.ctor) && in_class;
    }
    super::narrowed::narrow(l, class, &mut functions);
    dependencies(l, &mut functions, class, &roots, (in_class, pending))?;
    for (i, f) in functions.iter_mut().enumerate() {
        f.this &= !in_static.contains(&i);
    }
    super::narrowed::check(l, class, &mut functions)?;
    let methods = methods(l, class, &functions)?;
    Ok(Some(Lifting { td, class, functions, methods }))
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
            into: None,
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
                            into: None,
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
pub fn lambda(l: &Lower, t: TNodeId) -> Option<(TNodeId, TNodeId, SymbolId)> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[def] = l.tpd.items(stats) else { return None };
    let Tree::Closure { meth, .. } = *l.tpd.tree(expr) else { return None };
    let sym = l.tpd.sym(def)?;
    (matches!(l.tpd.tree(def), Tree::DefDef { .. }) && l.tpd.sym(meth) == Some(sym))
        .then_some((def, expr, sym))
}
