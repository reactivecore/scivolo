// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! dotty's `PickleQuotes` for term quotes whose holes capture nothing: `'{ e }` applied to its
//! `Quotes` becomes `quotes.asInstanceOf[QuoteUnpickler].unpickleExprV2[T](pickled, null, holes)`.

mod heal;
mod holes;
mod strings;
mod tags;

use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::{Span, TName};
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::pickle::{QuoteSource, pickle_positioned_quote};
use typer::tpd::{TNodeId, Tree, Walk, traverse};

const QUOTE: &str = "scala.quoted.runtime.Expr$.quote";
const SPLICE: &str = "scala.quoted.runtime.Expr$.splice";

/// A term quote applied to its `Quotes`: the quote's body, its type argument and `quotes`.
struct Quoted {
    body: TNodeId,
    tpe: TypeId,
    quotes: TNodeId,
}

/// Lowers the term quotes of unit `root`, owned by `owner`, outside its inline methods.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<()> {
    let mut result = Ok(());
    traverse(l, root, owner, &mut |l, t, owner| {
        if matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t).is_some_and(|s| has(l.u, s, INLINE)) {
            return Walk::Skip;
        }
        let Some(q) = quoted(l, t) else { return Walk::Children };
        match pickle(l, t, q, owner) {
            Ok(()) => Walk::Children,
            Err(e) => {
                result = Err(e);
                Walk::Stop
            }
        }
    });
    result
}

/// `t` as a term quote applied to its `Quotes`: `quote[T](body).apply(quotes)`.
fn quoted(l: &Lower, t: TNodeId) -> Option<Quoted> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let (body, tpe) = staged(l, qualifier, QUOTE)?;
    let &[quotes] = l.tpd.items(args) else { return None };
    Some(Quoted { body, tpe, quotes })
}

/// The argument and type argument of `t` if it calls `method[T](arg)`.
fn staged(l: &Lower, t: TNodeId, method: &str) -> Option<(TNodeId, TypeId)> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let Tree::TypeApply { fun: m, args: targs } = *l.tpd.tree(fun) else { return None };
    let sym = l.tpd.sym(m)?;
    if l.u.full_name(sym) != method {
        return None;
    }
    match (l.tpd.items(args), l.tpd.items(targs)) {
        (&[arg], &[targ]) => Some((arg, l.tpd.tpe(targ))),
        _ => None,
    }
}

/// Makes quote application `t` in `owner` the call unpickling the quote (dotty's `pickleAsTasty`).
fn pickle(l: &mut Lower, t: TNodeId, q: Quoted, owner: SymbolId) -> Result<()> {
    let span = l.tpd.span(q.body);
    if literal(l, q.body) {
        return unsupported(span, "quotes of a literal");
    }
    let tags = l.tpd.quote_tags(t).to_vec();
    let used = heal::heal(l, q.body, &tags);
    let mut holes = holes::extract(l, q.body)?;
    let (body, type_holes, tag_refs) = tags::encode(l, q.body, (&tags, &used), owner)?;
    holes.holes.extend(type_holes);
    let call = inline_call_trace(l, owner, span.to_synthetic());
    let bindings = l.tpd.list(&[]);
    let body_tpe = l.tpd.tpe(q.body);
    let root = l.tpd.node(
        Tree::Inlined { call: Some(call), bindings, expansion: body },
        span.to_synthetic(),
        Some(body_tpe),
        None,
    );
    let (base, file) = l.sources.files()[l.sources.index(span.in_own_source().start())];
    let source = QuoteSource::of(file.tasty_path().to_owned(), file, base);
    let bytes = pickle_positioned_quote((l.u, l.tpd, l.names), root, &holes.holes, &source)
        .or_else(|r| unsupported(span, format!("quotes with {r}")))?;
    let pickled = strings::pickled(l, &bytes)
        .or_else(|()| unsupported(span, "quotes whose TASTy takes several strings"))?;
    let types = types(l, &tag_refs);
    let term_holes = holes::lambda(l, &holes.contents, owner);
    let unpickler = l.u.class_ref("scala.quoted.runtime.QuoteUnpickler");
    let class = l.u.class_symbol(unpickler).expect("QuoteUnpickler");
    let meth =
        l.u.decl(class, "unpickleExprV2").ok().and_then(|d| d.first().copied()).expect("unpickleExprV2");
    let quotes = l.ensure_conforms(q.quotes, unpickler);
    let fun = l.select(quotes, meth, Span::NONE);
    let fun = l.positioned(fun);
    let info = l.u.info(meth);
    let Type::Poly { binder, params, result } = l.u.types().get(info).clone() else {
        panic!("unpickleExprV2 not polymorphic")
    };
    let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
    let instance = l.u.subst(result, binder, &names, &[q.tpe]);
    let targ = l.type_tree(q.tpe, Span::NONE);
    let fun = l.build().type_apply(fun, &[targ], instance, Span::NONE);
    let fun = l.positioned(fun);
    let expr = l.u.class_ref("scala.quoted.Expr");
    let expr = l.u.intern_type(Type::Applied(expr, vec![q.tpe]));
    let call = l.apply(fun, &[pickled, types, term_holes], expr, Span::NONE);
    let call = l.positioned(call);
    let call = l.with_span(call, span);
    l.copy_node(t, call);
    Ok(())
}

/// Whether `t` is a literal, past blocks and `Inlined` trees without statements.
fn literal(l: &Lower, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        Tree::Literal { .. } => true,
        Tree::Block { stats, expr } if l.tpd.items(stats).is_empty() => literal(l, expr),
        Tree::Inlined { bindings, expansion, .. } if l.tpd.items(bindings).is_empty() => {
            literal(l, expansion)
        }
        _ => false,
    }
}

/// A reference to the top-level class of `sym`, at `span` (dotty's `inlineCallTrace`).
fn inline_call_trace(l: &mut Lower, sym: SymbolId, span: Span) -> TNodeId {
    let mut top = sym;
    while let Some(o) = l.u.symbol(top).owner.filter(|&o| l.u.symbol(o).kind != Kind::Package) {
        top = o;
    }
    let tpe = l.u.intern_type(Type::Sym(None, top));
    let name = l.u.symbol(top).name.clone();
    let name = TName { name: l.names.intern(&name), is_type: true };
    l.tpd.node(Tree::Ident { name }, span, Some(tpe), Some(top))
}

/// `null` without tags, else the sequence literal of `refs` of `Type[? <: AnyKind]` elements.
fn types(l: &mut Lower, refs: &[TNodeId]) -> TNodeId {
    if refs.is_empty() {
        return l.null_literal(Span::NONE);
    }
    let [class, any_kind, seq] =
        ["quoted.Type", "AnyKind", "collection.immutable.Seq"].map(|c| l.u.class_ref(&format!("scala.{c}")));
    let bounds = l.u.intern_type(Type::Bounds(tasty::NOTHING, any_kind));
    let elem = l.u.intern_type(Type::Applied(class, vec![bounds]));
    let elemtpt = l.type_tree(elem, Span::NONE);
    let elems = l.tpd.list(refs);
    let tpe = l.u.intern_type(Type::Applied(seq, vec![elem]));
    let literal = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, Span::NONE, Some(tpe), None);
    l.positioned(literal)
}
