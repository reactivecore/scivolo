// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/QuotePatterns.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! Quote patterns encoded as the extractor patterns the pattern matcher handles (dotty's
//! `QuotePatterns.encode`): `quotes.asInstanceOf[QuoteMatching].ExprMatch.unapply[Bindings,
//! Results](using '{ shape }.apply(quotes))(Tuple(t$given1, ..., x1, ...))`.

mod shape;

use crate::definitions::type_ref;
use crate::phases::tree_copy::{Copier, copy};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::{Span, TName};
use tasty::tags::INLINE;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk, traverse};

/// Encodes the quote patterns of unit `root`, owned by `owner`, outside its inline methods.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<()> {
    let mut result = Ok(());
    traverse(l, root, owner, &mut |l, t, owner| {
        if matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t).is_some_and(|s| has(l.u, s, INLINE)) {
            return Walk::Skip;
        }
        if !matches!(l.tpd.tree(t), Tree::QuotePattern { .. }) {
            return Walk::Children;
        }
        match encode(l, t, owner) {
            Ok(()) => Walk::Children,
            Err(e) => {
                result = Err(e);
                Walk::Stop
            }
        }
    });
    result
}

/// Makes quote pattern `t` in `owner` its extractor pattern.
fn encode(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<()> {
    let Tree::QuotePattern { bindings, body, quotes } = *l.tpd.tree(t) else { unreachable!() };
    let span = l.tpd.span(t);
    let bindings = l.tpd.items(bindings).to_vec();
    let is_term = l.u.is_class_named(l.tpd.tpe(t), tasty::defn::QUOTED_EXPR);
    let tags = l.tpd.quote_tags(t).to_vec();
    let splices = shape::split(l, body)?;
    let shape = shape::with_bindings(l, body, &bindings, owner);
    let quotes_copy = copied(l, quotes, owner)?;
    let quoted =
        if is_term { quoted_term(l, shape, quotes_copy) } else { quoted_type(l, shape, quotes_copy) };
    let tagged = match *l.tpd.tree(quoted) {
        Tree::Apply { fun, .. } if !is_term => fun,
        _ => quoted,
    };
    l.tpd.set_quote_tags(tagged, tags);
    let mut patterns = Vec::new();
    for &b in &bindings {
        let binding = l.tpd.sym(b).expect("a type binding's symbol");
        let Some(given) = l.tpd.pattern_given(binding) else {
            return unsupported(span, "quote patterns without the givens of their type variables");
        };
        let tpe = l.u.info(given);
        let name = TName { name: l.names.intern("_"), is_type: false };
        let wild = l.tpd.node(Tree::Ident { name }, l.tpd.span(b), Some(tpe), None);
        let name = l.build().term_name(given);
        patterns.push(l.tpd.node(Tree::Bind { name, body: wild }, l.tpd.span(b), Some(tpe), Some(given)));
    }
    patterns.extend(splices);
    let types: Vec<TypeId> = patterns.iter().map(|&p| l.u.widen(l.tpd.tpe(p))).collect();
    let tuple = tuple_pattern(l, &patterns, &types, span)?;
    let (matcher, unapply) =
        if is_term { ("ExprMatch", "ExprMatchModule") } else { ("TypeMatch", "TypeMatchModule") };
    let matching = l.u.class_ref("scala.quoted.runtime.QuoteMatching");
    let quotes = l.ensure_conforms(quotes, matching);
    let class = l.u.class_symbol(matching).expect("QuoteMatching");
    let module = l.u.decl(class, matcher).ok().and_then(|d| d.first().copied()).expect("a matcher");
    let module = l.select(quotes, module, Span::NONE);
    let module = l.positioned(module);
    let class = l.u.decl(class, unapply).ok().and_then(|d| d.first().copied()).expect("a matcher class");
    let unapply = l.u.decl(class, "unapply").ok().and_then(|d| d.first().copied()).expect("unapply");
    let fun = l.select(module, unapply, Span::NONE);
    let fun = l.positioned(fun);
    let bound = nested_pairs(l, &bindings.iter().map(|&b| l.tpd.tpe(b)).collect::<Vec<_>>());
    let results = tuple_type(l, &types);
    let fun = poly_call(l, fun, &[bound, results], Span::NONE);
    let fun = l.positioned(fun);
    let (implicits, patterns) = (l.tpd.list(&[quoted]), l.tpd.list(&[tuple]));
    let tpe = l.tpd.tpe(t);
    let unapply = l.tpd.node(Tree::UnApply { fun, implicits, patterns }, Span::NONE, Some(tpe), None);
    let unapply = l.positioned(unapply);
    l.copy_node(t, unapply);
    Ok(())
}

/// `scala.quoted.runtime.Expr.quote[T](shape).apply(quotes)`.
fn quoted_term(l: &mut Lower, shape: TNodeId, quotes: TNodeId) -> TNodeId {
    let tpe = l.tpd.tpe(shape);
    let quote = runtime(l, "scala.quoted.runtime.Expr$", "quote", Span::NONE);
    let quote = poly_call(l, quote, &[tpe], Span::NONE);
    let method = l.u.widen(l.tpd.tpe(quote));
    let Some(result) = l.u.method(method).map(|m| m.result) else { panic!("quote") };
    let quote = l.apply(quote, &[shape], result, Span::NONE);
    let quote = l.positioned(quote);
    let class = l.u.class_symbol(result).expect("ContextFunction1");
    let apply = l.u.member(class, "apply")[0];
    let fun = l.select(quote, apply, Span::NONE);
    let fun = l.positioned(fun);
    let expr = l.u.applied(result).map(|(_, args)| args[1]).expect("Expr[T]");
    let call = l.apply(fun, &[quotes], expr, Span::NONE);
    l.positioned(call)
}

/// `scala.quoted.Type.of[shape](quotes)`.
fn quoted_type(l: &mut Lower, shape: TNodeId, quotes: TNodeId) -> TNodeId {
    let of = runtime(l, "scala.quoted.Type$", "of", Span::NONE);
    let info = l.u.info(l.tpd.sym(of).expect("Type.of"));
    let Some((binder, names, result)) = l.u.poly(info).map(|p| (p.binder, p.names(), p.result)) else {
        panic!("Type.of")
    };
    let tpe = l.tpd.tpe(shape);
    let instance = l.u.subst(result, binder, &names, &[tpe]);
    let fun = l.build().type_apply(of, &[shape], instance, Span::NONE);
    let fun = l.positioned(fun);
    let Some(result) = l.u.method(instance).map(|m| m.result) else { panic!("Type.of") };
    let call = l.apply(fun, &[quotes], result, Span::NONE);
    l.positioned(call)
}

/// The pattern of a tuple of `patterns` of types `types`: `EmptyTuple` or `TupleN.unapply`.
fn tuple_pattern(l: &mut Lower, patterns: &[TNodeId], types: &[TypeId], span: Span) -> Result<TNodeId> {
    if patterns.is_empty() {
        let module = empty_tuple(l);
        return Ok(l.ident(module, Span::NONE));
    }
    if patterns.len() > tasty::defn::MAX_TUPLE_ARITY {
        return unsupported(span, "quote patterns of more than 22 splices and type variables");
    }
    let unapply = runtime(l, &format!("scala.Tuple{}$", patterns.len()), "unapply", Span::NONE);
    let fun = poly_call(l, unapply, types, Span::NONE);
    let fun = l.positioned(fun);
    let tpe = tuple_type(l, types);
    let (implicits, patterns) = (l.tpd.list(&[]), l.tpd.list(patterns));
    let unapply = l.tpd.node(Tree::UnApply { fun, implicits, patterns }, Span::NONE, Some(tpe), None);
    Ok(l.positioned(unapply))
}

/// `(T1, ..., Tn)`, `EmptyTuple` for no types.
fn tuple_type(l: &mut Lower, types: &[TypeId]) -> TypeId {
    if types.is_empty() {
        let module = empty_tuple(l);
        return type_ref(l.u, module);
    }
    let class = l.u.class_ref(&format!("scala.Tuple{}", types.len()));
    l.u.intern_applied(class, types)
}

/// Object `scala.EmptyTuple`.
fn empty_tuple(l: &mut Lower) -> SymbolId {
    let class = l.u.class_ref("scala.EmptyTuple$");
    l.u.class_symbol(class).and_then(|c| l.u.source_module(c)).expect("EmptyTuple")
}

/// `KCons[T1, KCons[..., KNil]]` of `types` (dotty's `hkNestedPairsTypeTree`).
pub fn nested_pairs(l: &mut Lower, types: &[TypeId]) -> TypeId {
    let module = l.u.class_ref("scala.quoted.runtime.QuoteMatching$");
    let module = l.u.class_symbol(module).expect("QuoteMatching$");
    let [nil, cons] = ["KNil", "KCons"].map(|n| {
        let s = l.u.decl(module, n).ok().and_then(|d| d.first().copied()).expect("KNil and KCons");
        type_ref(l.u, s)
    });
    types.iter().rev().fold(nil, |acc, &t| l.u.intern_applied(cons, &[t, acc]))
}

/// A reference to method `name` of top-level object class `module`, through the object's `this`.
fn runtime(l: &mut Lower, module: &str, name: &str, span: Span) -> TNodeId {
    let class = l.u.class_ref(module);
    let class = l.u.class_symbol(class).unwrap_or_else(|| panic!("{module}"));
    let meth =
        l.u.decl(class, name).ok().and_then(|d| d.first().copied()).unwrap_or_else(|| panic!("{name}"));
    let this = l.u.intern_type(Type::Sym(None, class));
    let this = l.u.intern_type(Type::This(this));
    let tpe = l.u.intern_type(Type::Sym(Some(this), meth));
    l.build().ident(meth, tpe, span)
}

/// Reference `fun` to a polymorphic method without class type parameters applied to type
/// arguments `targs`.
fn poly_call(l: &mut Lower, fun: TNodeId, targs: &[TypeId], span: Span) -> TNodeId {
    let method = l.tpd.sym(fun).expect("a method reference");
    let info = l.u.info(method);
    let Some((binder, names, result)) = l.u.poly(info).map(|p| (p.binder, p.names(), p.result)) else {
        panic!("not polymorphic")
    };
    let instance = l.u.subst(result, binder, &names, targs);
    let instance = l.u.map_type(instance, &mut |u, x| match u.sym_of(x) {
        Some(s) if u.symbol(s).owner == Some(method) => {
            names.iter().position(|&n| u.name(n) == u.symbol(s).name).map(|i| targs[i])
        }
        _ => None,
    });
    let trees: Vec<TNodeId> = targs.iter().map(|&t| l.type_tree(t, Span::NONE)).collect();
    l.build().type_apply(fun, &trees, instance, span)
}

/// A copy of tree `t` in `owner`.
fn copied(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<TNodeId> {
    let mut m = Copier { syms: Default::default(), from: owner, to: owner };
    copy(l, &mut m, t)
}
