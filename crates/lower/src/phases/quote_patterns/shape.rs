// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/QuotePatterns.scala
//! The shape a quote pattern matches against: its splices made holes, its type variables
//! local `@patternType` types.

use super::{poly_call, runtime};
use crate::phases::pickle_quotes::{annotate_new, substitute};
use crate::phases::walk::children;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::{Span, TName};
use tasty::defn::PATTERNS;
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes each splice pattern of quote pattern body `body` a hole, `patternHole[T]`, or
/// `higherOrderHole[T](args)` with arguments; the patterns of the splices in tree order (dotty's
/// `splitQuotePattern`).
pub fn split(l: &mut Lower, body: TNodeId) -> Result<Vec<TNodeId>> {
    let (mut patterns, mut todo) = (Vec::new(), vec![body]);
    while let Some(t) = todo.pop() {
        let typed = match *l.tpd.tree(t) {
            Tree::Typed { expr, tpt } => Some((expr, tpt)),
            _ => None,
        };
        let is_splice = matches!(l.tpd.tree(t), Tree::SplicePattern { .. });
        let splice = match typed {
            Some((expr, tpt)) if is_plain_splice(l, expr) && !is_repeated(l, tpt) => expr,
            _ if is_splice => t,
            _ => {
                todo.extend(children(l.tpd, t).into_iter().rev());
                continue;
            }
        };
        let Tree::SplicePattern { body: pattern, typeargs, args } = *l.tpd.tree(splice) else {
            unreachable!()
        };
        if is_repeated(l, pattern) {
            return unsupported(l.tpd.span(splice), "repeated splice patterns");
        }
        patterns.push(pattern);
        let (tpe, span) = (l.tpd.tpe(splice), l.tpd.span(splice));
        let (typeargs, args) = (l.tpd.items(typeargs).to_vec(), l.tpd.items(args).to_vec());
        let hole = match (typeargs.is_empty(), args.is_empty()) {
            (true, true) => {
                let hole = runtime(l, PATTERNS, "patternHole", span);
                poly_call(l, hole, &[tpe], span)
            }
            (true, false) => {
                let hole = runtime(l, PATTERNS, "higherOrderHole", span);
                let hole = poly_call(l, hole, &[tpe], span);
                applied_to_seq(l, hole, &args, span)
            }
            (false, _) => {
                let typeargs: Vec<_> = typeargs.iter().map(|&a| l.tpd.tpe(a)).collect();
                let pairs = super::nested_pairs(l, &typeargs);
                let hole = runtime(l, PATTERNS, "higherOrderHoleWithTypes", span);
                let hole = poly_call(l, hole, &[tpe, pairs], span);
                applied_to_seq(l, hole, &args, span)
            }
        };
        l.copy_node(t, hole);
    }
    Ok(patterns)
}

/// `body` after local type definitions standing for type variables `bindings`, in `owner`, each
/// `@patternType`, `@fromAbove` if its binding is (dotty's `shape1` of `encode`).
pub fn with_bindings(l: &mut Lower, body: TNodeId, bindings: &[TNodeId], owner: SymbolId) -> TNodeId {
    if bindings.is_empty() {
        return body;
    }
    let (mut map, mut news) = (FxHashMap::default(), Vec::new());
    for &b in bindings {
        let old = l.tpd.sym(b).expect("a type binding's symbol");
        let name = l.u.symbol(old).name.clone();
        let new = l.u.define(crate::definitions::symbol(&name, Kind::Type, owner, Flags::default()));
        let (old_ref, new_ref) =
            (l.u.intern_type(Type::Sym(None, old)), l.u.intern_type(Type::Sym(None, new)));
        map.insert(old_ref, new_ref);
        news.push((b, old, new));
    }
    let mut defs = Vec::new();
    for (b, old, new) in news {
        let info = l.u.info(old);
        let info = l.u.map_type(info, &mut |_, x| map.get(&x).copied());
        l.u.set_info(new, info);
        let span = l.tpd.span(b);
        let [pattern_type, from_above] = ["patternType", "fromAbove"].map(|n| patterns_class(l, n));
        annotate_new(l, new, pattern_type, span);
        if l.u.annotations(old).contains(&from_above) {
            annotate_new(l, new, from_above, span);
        }
        let name = TName { name: l.names.intern(&l.u.symbol(new).name.clone()), is_type: true };
        let at = Span::at(span.point());
        let rhs = l.type_tree(info, at);
        let tpe = l.u.intern_type(Type::Sym(None, new));
        defs.push(l.tpd.node(Tree::TypeDef { name, rhs }, at, Some(tpe), Some(new)));
    }
    substitute(l, body, &map);
    let tpe = l.tpd.tpe(body);
    let block = l.block(&defs, body, Span::NONE);
    l.tpd.get_mut(block).tpe = Some(tpe);
    l.positioned(block)
}

/// Annotation class `name` of object `scala.quoted.runtime.Patterns`.
fn patterns_class(l: &mut Lower, name: &str) -> SymbolId {
    let module = l.u.class_ref(PATTERNS);
    let module = l.u.class_symbol(module).expect("Patterns$");
    l.u.decl(module, name).ok().and_then(|d| d.first().copied()).expect("a Patterns annotation")
}

/// `fun(SeqLiteral(args): Seq[Any])`, of the result type of `fun`.
fn applied_to_seq(l: &mut Lower, fun: TNodeId, args: &[TNodeId], span: Span) -> TNodeId {
    let [any, seq] = ["scala.Any", "scala.collection.immutable.Seq"].map(|c| l.u.class_ref(c));
    let elemtpt = l.type_tree(any, Span::NONE);
    let elems = l.tpd.list(args);
    let tpe = l.u.intern_applied(seq, &[any]);
    let literal = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, Span::NONE, Some(tpe), None);
    let literal = l.positioned(literal);
    let method = l.u.widen(l.tpd.tpe(fun));
    let Some(result) = l.u.method(method).map(|m| m.result) else { panic!("a hole method") };
    l.apply(fun, &[literal], result, span)
}

/// Whether `t` is a splice pattern without type or term arguments.
fn is_plain_splice(l: &Lower, t: TNodeId) -> bool {
    matches!(*l.tpd.tree(t), Tree::SplicePattern { typeargs, args, .. }
        if l.tpd.items(typeargs).is_empty() && l.tpd.items(args).is_empty())
}

/// Whether the type of `t` is a repeated parameter type.
fn is_repeated(l: &mut Lower, t: TNodeId) -> bool {
    let tpe = l.u.widen(l.tpd.tpe(t));
    let class = l.u.applied(tpe).map(|(c, _)| c);
    class.is_some_and(|c| l.u.is_class_named(c, tasty::defn::REPEATED))
}
