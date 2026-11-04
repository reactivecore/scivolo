// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//   compiler/src/dotty/tools/dotc/transform/ReifiedReflect.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! A captured method applied in a nested quote, `f[T](a)`, and an assignment to a captured `var`,
//! `x = a`, as the splice building the tree by reflection on the captured `Expr`:
//! `${ asExpr(Apply(TypeApply(asTerm(f$1), List(Inferred(TypeRepr.of[T]))), List(asTerm('{ a })))) }`
//! (dotty's `transformCapturedApplication`, `transformSplicedAssign`).

use super::reflect::{Reflect, call, member, type_applied};
use super::splicing::Splicer;
use super::staged_trees::{quoted, spliced};
use crate::Lower;
use crate::phases::walk::subtrees;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

const QUOTES: &str = "a reference to Quotes";

impl Splicer<'_> {
    /// The splice replacing `t` at quote level `level` in `owner` if it applies a captured method
    /// to arguments.
    pub fn captured_application(
        &mut self,
        l: &mut Lower,
        t: TNodeId,
        level: i32,
        owner: SymbolId,
    ) -> Option<TNodeId> {
        let mut clauses = Vec::new();
        let mut f = t;
        while let Tree::Apply { fun, args } | Tree::TypeApply { fun, args } = *l.tpd.tree(f) {
            let terms = matches!(l.tpd.tree(f), Tree::Apply { .. });
            clauses.push((l.tpd.items(args).to_vec(), terms));
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
            let mut acc = r.as_term(l, fun).expect(QUOTES);
            for (args, terms) in clauses {
                let trees: Vec<TNodeId> = args
                    .into_iter()
                    .map(|a| {
                        self.walk(l, (a, Span::NONE), level - 1, anon);
                        let tpe = l.tpd.tpe(a);
                        if terms { quoted_term(l, r, a, owner) } else { inferred(l, r, tpe) }
                    })
                    .collect();
                let names = if terms { ("Apply", "Term") } else { ("TypeApply", "TypeTree") };
                acc = reflected(l, r, names, acc, &trees);
            }
            r.as_expr(l, tpe, acc).expect(QUOTES)
        }))
    }

    /// The splice replacing `t` at quote level `level` in `owner` if it assigns a captured `var`.
    pub fn captured_assign(
        &mut self,
        l: &mut Lower,
        t: TNodeId,
        level: i32,
        owner: SymbolId,
    ) -> Option<TNodeId> {
        let Tree::Assign { lhs, rhs } = *l.tpd.tree(t) else { return None };
        let s = self.captured_ident(l, lhs)?;
        let quotes = self.quotes?;
        let tpe = l.tpd.tpe(t);
        let param = self.param(l, lhs, s, None);
        Some(spliced(l, tpe, owner, |l, _, anon| {
            let r = Reflect { quotes, owner: anon };
            let lhs = l.ident(param, Span::NONE);
            let lhs = r.as_term(l, lhs).expect(QUOTES);
            self.walk(l, (rhs, Span::NONE), level, anon);
            let rhs = quoted_term(l, r, rhs, owner);
            let assign = r.method(l, "Assign", "apply").expect(QUOTES);
            let assign = call(l, assign, &[lhs, rhs]);
            r.as_expr(l, tpe, assign).expect(QUOTES)
        }))
    }
}

/// `asTerm('{ a })` of term `a` moved from `owner` into the splice of `r`.
fn quoted_term(l: &mut Lower, r: Reflect, a: TNodeId, owner: SymbolId) -> TNodeId {
    reown(l, a, owner, r.owner);
    let quotes = r.copy(l).expect(QUOTES);
    let q = quoted(l, a, quotes);
    r.as_term(l, q).expect(QUOTES)
}

/// `quotes.reflect.Inferred.apply(quotes.reflect.TypeRepr.of[tpe](Type.of[tpe](quotes)))`.
fn inferred(l: &mut Lower, r: Reflect, tpe: TypeId) -> TNodeId {
    let quotes = r.copy(l).expect(QUOTES);
    let module = l.u.class_ref("scala.quoted.Type$");
    let module = l.u.class_symbol(module).expect("scala.quoted.Type");
    let of = l.u.decl(module, "of").ok().and_then(|d| d.first().copied()).expect("Type.of");
    let of = l.ident(of, Span::NONE);
    let of = type_applied(l, of, tpe);
    let tag = call(l, of, &[quotes]);
    let repr = r.method(l, "TypeRepr", "of").expect(QUOTES);
    let repr = type_applied(l, repr, tpe);
    let repr = call(l, repr, &[tag]);
    let apply = r.method(l, "Inferred", "apply").expect(QUOTES);
    call(l, apply, &[repr])
}

/// `quotes.reflect.module.apply(fun, List.apply[quotes.reflect.elem](args*))`.
fn reflected(
    l: &mut Lower,
    r: Reflect,
    (module, elem): (&str, &str),
    fun: TNodeId,
    args: &[TNodeId],
) -> TNodeId {
    let apply = r.method(l, module, "apply").expect(QUOTES);
    let reflect = r.this(l).expect(QUOTES);
    let prefix = l.tpd.tpe(reflect);
    let class = l.u.widen(prefix);
    let class = l.u.class_symbol(class).expect("reflectModule");
    let elem = l.u.member(class, elem).into_iter().find(|&m| l.u.symbol(m).kind == Kind::Type);
    let elem = l.u.intern_type(Type::Sym(Some(prefix), elem.expect("a reflect type")));
    let list = list_of(l, elem, args);
    call(l, apply, &[fun, list])
}

/// `List.apply[elem](elems*)` (dotty's `mkList`).
fn list_of(l: &mut Lower, elem: TypeId, elems: &[TNodeId]) -> TNodeId {
    let module = l.u.class_ref("scala.collection.immutable.List$");
    let class = l.u.class_symbol(module).expect("List");
    let module = l.u.source_module(class).expect("object List");
    let list = l.ident(module, Span::NONE);
    let apply = member(l, list, "apply");
    let fun = type_applied(l, apply, elem);
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
