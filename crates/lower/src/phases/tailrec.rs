// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TailRec.scala
//! dotty's `TailRec`: a method that cannot be overridden and calls itself in tail position
//! becomes a loop, `{ var p$tailLocal1 = p; while true do tailLabel1: { return rhs } }`, its
//! tail calls assignments of the variables and a jump to the label.

use super::lazy_trees::{block, local, return_, val_def};
use super::local_free::{enclosing_class, is_term};
use super::tail_calls::TailCalls;
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::{has, is_effectively_final, is_static};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::FIELDaccessor;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree, Walk, traverse_up};

/// Makes the tail recursive methods of class definition `td` loops.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Some(Template { body, .. }) = l.tpd.class_template(td) else { return };
    for stat in l.tpd.items(body).to_vec() {
        if let Some(m) = l.tpd.sym(stat).filter(|&m| is_effectively_final(l.u, class, m)) {
            rewrite(l, stat, class, m, false);
        }
    }
}

/// Before `LambdaLift`, the methods under `root`, owned by `owner`, in dotty's order, inner ones
/// first: local tail recursive methods made loops, the variables of members' named.
pub fn name_vars(l: &mut Lower, root: TNodeId, owner: SymbolId) {
    let mut defs = Vec::new();
    let leave = &mut |l: &mut Lower, t, _| {
        if matches!(l.tpd.tree(t), Tree::DefDef { .. }) {
            defs.push(t);
        }
        std::ops::ControlFlow::Continue(())
    };
    traverse_up(l, root, owner, &mut |_, _, _| Walk::Children, leave);
    for t in defs {
        let Some(m) = l.tpd.sym(t) else { continue };
        let Some(o) = l.u.symbol(m).owner else { continue };
        let local = is_term(l, o);
        let class = enclosing_class(l, o);
        if local || is_effectively_final(l.u, class, m) {
            let vars = rewrite(l, t, class, m, !local);
            if local {
                l.tail_locals.extend(vars);
            }
        }
    }
}

/// Makes method `m` of `class` defined by `stat` a loop if it calls itself in tail position; if
/// `dry`, only names the variables it would make, which `rewrite` takes later; the variables
/// made.
fn rewrite(l: &mut Lower, stat: TNodeId, class: SymbolId, m: SymbolId, dry: bool) -> Vec<SymbolId> {
    let Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } = l.tpd.tree(stat).clone() else {
        return Vec::new();
    };
    if has(l.u, m, FIELDaccessor) || l.u.is_constructor(m) {
        return Vec::new();
    }
    let params: Vec<_> = l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c).to_vec()).collect();
    let params = params
        .into_iter()
        .filter(|&p| matches!(l.tpd.tree(p), Tree::ValDef { .. }))
        .filter_map(|p| l.tpd.sym(p))
        .collect();
    let mut calls = TailCalls::new(m, class, params);
    calls.dry = dry;
    calls.walk(l, rhs, true);
    if dry {
        l.tail_vars.extend(calls.vars.iter().copied());
        l.tail_vars.extend(calls.this_var.map(|v| (m, v)));
        return Vec::new();
    }
    let Some(label) = calls.label else { return Vec::new() };
    substitute(l, rhs, &calls);
    let mut defs = Vec::new();
    if let Some(v) = calls.this_var {
        let this = l.this(class, Span::NONE);
        defs.push(val_def(l, v, this));
    }
    for &(param, var) in &calls.vars {
        let read = l.ident(param, Span::NONE);
        defs.push(val_def(l, var, read));
    }
    let ret = return_(l, m, rhs);
    let unit = l.u.class_ref("scala.Unit");
    let labeled = l.tpd.node(Tree::Labeled { expr: ret }, Span::NONE, Some(unit), Some(label));
    let body = l.positioned(labeled);
    let cond = l.boolean_literal(true);
    let local = l.u.symbol(m).owner.is_some_and(|o| is_term(l, o));
    let tpe = if local { l.u.class_ref("scala.Nothing") } else { unit };
    let looped = l.tpd.node(Tree::WhileDo { cond, body }, Span::NONE, Some(tpe), None);
    let looped = l.positioned(looped);
    l.endless_loops.insert(looped);
    let rhs = block(l, &defs, looped);
    l.tpd.get_mut(stat).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    calls.vars.iter().map(|&(_, v)| v).chain(calls.this_var).collect()
}

impl TailCalls {
    /// The label `tailLabelN` the tail calls jump to.
    pub fn label(&mut self, l: &mut Lower) -> SymbolId {
        if let Some(label) = self.label {
            return label;
        }
        let name = fresh(l, "tailLabel");
        let unit = l.u.class_ref("scala.Unit");
        let label = local(l, self.method, &name, unit, false);
        self.label = Some(label);
        label
    }

    /// The variable `$this$tailLocalN` for the receiver.
    pub fn this_var(&mut self, l: &mut Lower) -> SymbolId {
        if let Some(v) = self.this_var {
            return v;
        }
        let v = l.tail_vars.remove(&self.method).unwrap_or_else(|| {
            let name = fresh(l, "$this$tailLocal");
            let tpe = l.u.this_type(self.class);
            local(l, self.method, &name, tpe, true)
        });
        self.this_var = Some(v);
        v
    }

    /// The variable `p$tailLocalN` for parameter `param`, put first among `vars`.
    pub fn param_var(&mut self, l: &mut Lower, param: SymbolId) -> SymbolId {
        if let Some(&(_, v)) = self.vars.iter().find(|(p, _)| *p == param) {
            return v;
        }
        let info = l.u.info(param);
        let v = match l.tail_vars.remove(&param) {
            Some(v) => {
                l.u.set_info(v, info);
                v
            }
            None => {
                let name = fresh(l, &format!("{}$tailLocal", l.u.symbol(param).name));
                local(l, self.method, &name, info, true)
            }
        };
        self.vars.insert(0, (param, v));
        v
    }
}

/// `baseN`, `N` counting the names made from `base` in the unit.
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let name = l.fresh.next(l.names, base);
    l.names.text(name).to_owned()
}

/// Makes the references under `t` to the rewritten parameters, and to `this` but under `super`,
/// read their variables.
fn substitute(l: &mut Lower, t: TNodeId, calls: &TailCalls) {
    let trees = subtrees(l.tpd, t);
    let under_super: FxHashSet<TNodeId> = trees
        .iter()
        .filter_map(|&s| match *l.tpd.tree(s) {
            Tree::Super { qual, .. } => Some(qual),
            _ => None,
        })
        .collect();
    let bases = l.u.linearization(calls.class);
    for s in trees {
        let sym = l.tpd.sym(s);
        let span = l.tpd.span(s);
        let param_var = calls.vars.iter().find(|(p, _)| Some(*p) == sym).map(|&(_, v)| v);
        let new = match (l.tpd.tree(s), param_var, calls.this_var, sym) {
            (Tree::Ident { .. }, Some(v), _, _) => l.ident(v, span),
            (Tree::Ident { .. }, None, Some(v), Some(m)) if is_member(l, &bases, m) => {
                let this = l.ident(v, span);
                l.select(this, m, span)
            }
            (Tree::This { .. }, _, Some(v), _) if sym == Some(calls.class) && !under_super.contains(&s) => {
                l.ident(v, span)
            }
            _ => continue,
        };
        l.copy_node(s, new);
    }
}

/// Whether `s` is a member of a class of `bases` an identifier reaches through `this`.
fn is_member(l: &Lower, bases: &[SymbolId], s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    sym.owner.is_some_and(|o| bases.contains(&o))
        && matches!(sym.kind, Kind::Def | Kind::Val)
        && !l.u.is_module(s)
        && !is_static(l.u, s)
}
