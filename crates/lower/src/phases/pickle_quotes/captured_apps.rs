// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//   compiler/src/dotty/tools/dotc/transform/ReifiedReflect.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! A captured method applied in a nested quote, `f(a)`, as the splice applying its `Expr` by
//! reflection: `${ asExpr(Apply(asTerm(f$1), List(asTerm('{ a })))) }` (dotty's
//! `transformCapturedApplication`).

use super::reflect::{Reflect, call, member};
use super::splicing::Splicer;
use super::staged_trees::{quoted, spliced};
use crate::Lower;
use crate::phases::walk::subtrees;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Splicer<'_> {
    /// The splice replacing `t` at quote level `level` in `owner` if it applies a captured method
    /// to term arguments.
    pub fn captured_application(
        &mut self,
        l: &mut Lower,
        t: TNodeId,
        level: i32,
        owner: SymbolId,
    ) -> Option<TNodeId> {
        let mut clauses = Vec::new();
        let mut f = t;
        while let Tree::Apply { fun, args } = *l.tpd.tree(f) {
            clauses.push(l.tpd.items(args).to_vec());
            f = fun;
        }
        let s = self.captured_ident(l, f).filter(|_| !clauses.is_empty())?;
        let quotes = self.quotes?;
        clauses.reverse();
        let tpe = l.tpd.tpe(t);
        let param = self.param(l, f, s, Some(tasty::ANY));
        Some(spliced(l, tpe, owner, |l, _, anon| {
            let r = Reflect { quotes, owner: anon };
            let fun = l.ident(param, Span::NONE);
            let mut acc = r.as_term(l, fun).expect("a reference to Quotes");
            for args in clauses {
                let terms: Vec<TNodeId> = args
                    .into_iter()
                    .map(|a| {
                        self.walk(l, (a, Span::NONE), level - 1, anon);
                        reown(l, a, owner, anon);
                        let quotes = r.copy(l).expect("a reference to Quotes");
                        let q = quoted(l, a, quotes);
                        r.as_term(l, q).expect("a reference to Quotes")
                    })
                    .collect();
                acc = reflected_apply(l, r, acc, &terms);
            }
            r.as_expr(l, tpe, acc).expect("a reference to Quotes")
        }))
    }
}

/// `quotes.reflect.Apply.apply(fun, List.apply[quotes.reflect.Term](args*))`.
fn reflected_apply(l: &mut Lower, r: Reflect, fun: TNodeId, args: &[TNodeId]) -> TNodeId {
    let apply = r.method(l, "Apply", "apply").expect("a reference to Quotes");
    let reflect = r.this(l).expect("a reference to Quotes");
    let prefix = l.tpd.tpe(reflect);
    let module = l.u.widen(prefix);
    let class = l.u.class_symbol(module).expect("reflectModule");
    let term = l.u.member(class, "Term").into_iter().find(|&m| l.u.symbol(m).kind == Kind::Type);
    let term = l.u.intern_type(Type::Sym(Some(prefix), term.expect("reflect.Term")));
    let list = list_of(l, term, args);
    call(l, apply, &[fun, list])
}

/// `List.apply[elem](elems*)` (dotty's `mkList`).
fn list_of(l: &mut Lower, elem: TypeId, elems: &[TNodeId]) -> TNodeId {
    let module = l.u.class_ref("scala.collection.immutable.List$");
    let class = l.u.class_symbol(module).expect("List");
    let module = l.u.source_module(class).expect("object List");
    let list = l.ident(module, Span::NONE);
    let apply = member(l, list, "apply");
    let info = l.u.widen(l.tpd.tpe(apply));
    let Type::Poly { binder, params, result } = l.u.types().get(info).clone() else { panic!("List.apply") };
    let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
    let instance = l.u.subst(result, binder, &names, &[elem]);
    let targ = l.type_tree(elem, Span::NONE);
    let fun = l.build().type_apply(apply, &[targ], instance, Span::NONE);
    let fun = l.positioned(fun);
    let repeated = l.u.class_ref("scala.<repeated>");
    let repeated = l.u.intern_type(Type::Applied(repeated, vec![elem]));
    let elemtpt = l.type_tree(elem, Span::NONE);
    let items = l.tpd.list(elems);
    let seq = l.u.class_ref("scala.collection.immutable.Seq");
    let seq = l.u.intern_type(Type::Applied(seq, vec![elem]));
    let literal = l.tpd.node(Tree::SeqLiteral { elems: items, elemtpt }, Span::NONE, Some(seq), None);
    let literal = l.positioned(literal);
    let tpt = l.type_tree(repeated, Span::NONE);
    let typed = l.tpd.node(Tree::Typed { expr: literal, tpt }, Span::NONE, Some(repeated), None);
    let typed = l.positioned(typed);
    call(l, fun, &[typed])
}

/// Makes `to` the owner of the definitions under `t` that `from` owns.
fn reown(l: &mut Lower, t: TNodeId, from: SymbolId, to: SymbolId) {
    for s in subtrees(l.tpd, t) {
        let defines = matches!(l.tpd.tree(s), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. });
        if let Some(d) = l.tpd.sym(s).filter(|&d| defines && l.u.symbol(d).owner == Some(from)) {
            l.u.set_owner(d, to);
        }
    }
}
