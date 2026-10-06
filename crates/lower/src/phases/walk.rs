//! The subtrees of typed trees, and the symbols trees refer to.

use rustc_hash::FxHashSet;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tpd, Tree};

/// The direct subtrees of `t`, in order.
pub fn children(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let list = |out: &mut Vec<TNodeId>, l| out.extend_from_slice(tpd.items(l));
    match *tpd.tree(t) {
        Tree::Ident { .. } | Tree::Literal { .. } | Tree::ClassLiteral { .. } | Tree::TypeTree { .. } => {}
        Tree::Select { qualifier: a, .. }
        | Tree::This { qual: a }
        | Tree::Super { qual: a, .. }
        | Tree::New { tpt: a }
        | Tree::Labeled { expr: a }
        | Tree::SingletonTypeTree { reference: a }
        | Tree::ByNameTypeTree { result: a }
        | Tree::Bind { body: a, .. }
        | Tree::TypeDef { rhs: a, .. } => out.push(a),
        Tree::Apply { fun, args } | Tree::TypeApply { fun, args } => {
            out.push(fun);
            list(&mut out, args);
        }
        Tree::AppliedTypeTree { tpt, args } => {
            out.push(tpt);
            list(&mut out, args);
        }
        Tree::Typed { expr: a, tpt: b }
        | Tree::Assign { lhs: a, rhs: b }
        | Tree::Return { expr: a, from: b }
        | Tree::Annotated { arg: a, annot: b }
        | Tree::WhileDo { cond: a, body: b } => {
            out.extend([a, b]);
        }
        Tree::Block { stats, expr } => {
            list(&mut out, stats);
            out.push(expr);
        }
        Tree::Closure { env, meth, tpt } => {
            list(&mut out, env);
            out.push(meth);
            out.extend(tpt);
        }
        Tree::SeqLiteral { elems, elemtpt } => {
            list(&mut out, elems);
            out.push(elemtpt);
        }
        Tree::If { cond, thenp, elsep } => out.extend([cond, thenp, elsep]),
        Tree::CaseDef { pat, guard, body } => {
            out.extend([Some(pat), guard, Some(body)].into_iter().flatten())
        }
        Tree::Try { expr, cases, finalizer } => {
            out.push(expr);
            list(&mut out, cases);
            out.extend(finalizer);
        }
        Tree::Match { selector, cases } => {
            out.push(selector);
            list(&mut out, cases);
        }
        Tree::Alternative { trees } => list(&mut out, trees),
        Tree::UnApply { fun, implicits, patterns } => {
            out.push(fun);
            list(&mut out, implicits);
            list(&mut out, patterns);
        }
        Tree::TypeBoundsTree { lo, hi, alias } => {
            out.extend([Some(lo), Some(hi), alias].into_iter().flatten())
        }
        Tree::ValDef { tpt, rhs, .. } => out.extend([Some(tpt), rhs].into_iter().flatten()),
        Tree::DefDef { paramss, tpt, rhs, .. } => {
            for &c in tpd.clause_items(paramss) {
                list(&mut out, c);
            }
            out.extend([Some(tpt), rhs].into_iter().flatten());
        }
        Tree::Template { constr, parents, self_, body } => {
            out.push(constr);
            list(&mut out, parents);
            out.extend(self_);
            list(&mut out, body);
        }
        Tree::Import { expr, selectors } => {
            out.push(expr);
            list(&mut out, selectors);
        }
        Tree::ImportSelector { imported, renamed, bound } => {
            out.extend([Some(imported), renamed, bound].into_iter().flatten());
        }
        Tree::PackageDef { pid, stats } => {
            out.push(pid);
            list(&mut out, stats);
        }
    }
    out
}

/// `t` and the trees in it, outer trees first.
pub fn subtrees(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    let mut out = vec![t];
    let mut i = 0;
    while i < out.len() {
        out.extend(children(tpd, out[i]));
        i += 1;
    }
    out
}

/// Whether `a` and `b` are the same identifiers, selections, literals, applications or type
/// applications (dotty's `===`).
pub fn same(tpd: &Tpd, a: TNodeId, b: TNodeId) -> bool {
    match (tpd.tree(a), tpd.tree(b)) {
        (Tree::Ident { .. }, Tree::Ident { .. }) => tpd.sym(a) == tpd.sym(b),
        (&Tree::Select { qualifier: q1, .. }, &Tree::Select { qualifier: q2, .. }) => {
            tpd.sym(a) == tpd.sym(b) && same(tpd, q1, q2)
        }
        (Tree::Literal { value: c1 }, Tree::Literal { value: c2 }) => c1 == c2,
        (&Tree::Apply { fun: f1, args: a1 }, &Tree::Apply { fun: f2, args: a2 }) => {
            let (a1, a2) = (tpd.items(a1), tpd.items(a2));
            same(tpd, f1, f2) && a1.len() == a2.len() && a1.iter().zip(a2).all(|(&x, &y)| same(tpd, x, y))
        }
        (&Tree::TypeApply { fun: f1, args: a1 }, &Tree::TypeApply { fun: f2, args: a2 }) => {
            let (a1, a2) = (tpd.items(a1), tpd.items(a2));
            same(tpd, f1, f2)
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(&x, &y)| tpd.tpe(x) == tpd.tpe(y))
        }
        _ => false,
    }
}

/// The symbols that the identifiers and selections in `t` refer to.
pub fn references(tpd: &Tpd, t: TNodeId) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        if matches!(tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. }) {
            out.extend(tpd.sym(t));
        }
        todo.extend(children(tpd, t));
    }
    out
}
