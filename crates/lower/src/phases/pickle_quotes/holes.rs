// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//! The term holes of a quote: its splices without captured references, and the `termHoles`
//! lambda calling their contents.

use super::{QUOTE, SPLICE, bound, splicing, staged};
use crate::definitions::symbol;
use crate::phases::lift_names::fresh;
use crate::phases::walk::{children, subtrees};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{MatchKind, TNodeId, Tree};
use typer::unpickle::QuoteHole;

/// The holes of a quote and, by index, the contents of their splices with the parameters their
/// hole's arguments bind.
pub struct Holes {
    pub holes: Vec<QuoteHole>,
    pub contents: Vec<(TNodeId, Vec<SymbolId>)>,
}

/// Makes each splice of quote body `body` in `owner` an `Inlined` tree around a hole (dotty's
/// `Splicing` and `extractHolesContents`); refuses nested quotes and the captured references
/// `splicing` leaves.
pub fn extract(l: &mut Lower, body: TNodeId, owner: SymbolId) -> Result<Holes> {
    let (mut splices, mut defined, mut refs) = (Vec::new(), FxHashSet::default(), Vec::new());
    let mut todo = vec![body];
    while let Some(t) = todo.pop() {
        if staged(l, t, QUOTE).is_some() || matches!(l.tpd.tree(t), Tree::QuotePattern { .. }) {
            return unsupported(l.tpd.span(t), "nested quotes");
        }
        if let Some((content, _)) = staged(l, t, SPLICE) {
            splices.push((t, content));
            continue;
        }
        match l.tpd.tree(t) {
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Bind { .. } => {
                defined.extend(l.tpd.sym(t));
            }
            Tree::Ident { .. } | Tree::This { .. } => refs.extend(l.tpd.sym(t).map(|s| (t, s))),
            _ => {}
        }
        todo.extend(children(l.tpd, t).into_iter().rev());
    }
    for (t, s) in refs {
        if !defined.contains(&s) && is_local(l, s) {
            return unsupported(l.tpd.span(t), "captured reference in a quote");
        }
    }
    let mut holes = Holes { holes: Vec::new(), contents: Vec::new() };
    for (index, (splice, content)) in splices.into_iter().enumerate() {
        let captures = splicing::capture(l, content, &defined, owner);
        let start = l.tpd.span(content).start_pos();
        for &a in &captures.args {
            if !l.tpd.span(a).exists() {
                l.tpd.get_mut(a).span = start;
            }
        }
        if let Some(&c) =
            subtrees(l.tpd, content).iter().find(|&&c| l.tpd.sym(c).is_some_and(|s| defined.contains(&s)))
        {
            return unsupported(l.tpd.span(c), "captured reference in a quote");
        }
        let tpe = l.tpd.get(splice).tpe;
        let span = l.tpd.span(content).to_synthetic();
        let node = l.tpd.node(Tree::TypeTree { inferred: false }, span, tpe, None);
        let bindings = l.tpd.list(&[]);
        let s = l.tpd.get_mut(splice);
        (s.tree, s.sym, s.span) = (Tree::Inlined { call: None, bindings, expansion: node }, None, span);
        holes.holes.push(QuoteHole { node, index: index as u32, args: captures.args });
        holes.contents.push((content, captures.params));
    }
    Ok(holes)
}

/// Whether `s` is a term or type local to a method or value, not a member.
fn is_local(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    sym.kind != Kind::Package
        && sym.owner.is_some_and(|o| !matches!(l.u.symbol(o).kind, Kind::Class | Kind::Package))
}

/// `null` without holes, else `(idx, contents, quotes) => idx match { case i => content_i(quotes) }`
/// in `owner`, `content_i` its parameters bound to `contents`, the only case's body without a
/// match; each tree spanned as dotty's constructors span it, from its children.
pub fn lambda(l: &mut Lower, contents: &[(TNodeId, Vec<SymbolId>)], owner: SymbolId) -> TNodeId {
    if contents.is_empty() {
        return l.null_literal(Span::NONE);
    }
    let [int, any, quotes, seq, expr, function] =
        ["Int", "Any", "quoted.Quotes", "collection.immutable.Seq", "quoted.Expr", "Function3"]
            .map(|c| l.u.class_ref(&format!("scala.{c}")));
    let seq = l.u.intern_type(Type::Applied(seq, vec![any]));
    let result = l.u.intern_type(Type::Applied(expr, vec![any]));
    let params =
        [("idx", int), ("contents", seq), ("quotes", quotes)].map(|(n, t)| (fresh(l, n), t)).to_vec();
    let anon = l.u.define(symbol("$anonfun", Kind::Def, owner, Flags::default().with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
    l.u.set_info(anon, info);
    for (c, _) in contents {
        reown(l, *c, anon);
    }
    let def = l.forwarding_def(anon, Span::NONE, |l, args, _| {
        let [idx, seq_param, quotes_param] = [0, 1, 2].map(|i| l.tpd.sym(args[i]).expect("a parameter"));
        let (mut at, mut bound_at) = (Span::NONE, Span::NONE);
        let rhss: Vec<TNodeId> = contents
            .iter()
            .map(|(c, params)| {
                let c = bound::bound(l, (*c, params), (seq_param, anon), &mut bound_at);
                applied(l, c, quotes_param, quotes, &mut at)
            })
            .collect();
        if let &[rhs] = &rhss[..] {
            return rhs;
        }
        let tpe = rhss.iter().map(|&r| l.tpd.tpe(r)).reduce(|a, b| l.u.lub(a, b)).expect("a case");
        let cases: Vec<TNodeId> = rhss
            .into_iter()
            .enumerate()
            .map(|(i, body)| {
                let pat = l.int_literal(i as i32);
                let case = l.tpd.node(Tree::CaseDef { pat, guard: None, body }, Span::NONE, Some(tpe), None);
                l.positioned(case)
            })
            .collect();
        let cases = l.tpd.list(&cases);
        let unchecked = l.u.class_ref("scala.unchecked");
        let annotated = l.u.intern_type(Type::Annotated(int, unchecked));
        let expr = l.ident(idx, Span::NONE);
        let tpt = l.type_tree(annotated, Span::NONE);
        let selector = l.tpd.node(Tree::Typed { expr, tpt }, Span::NONE, Some(annotated), None);
        let selector = Some(l.positioned(selector));
        let m =
            l.tpd.node(Tree::Match { selector, cases, kind: MatchKind::Plain }, Span::NONE, Some(tpe), None);
        l.positioned(m)
    });
    let def = l.positioned(def);
    let function = l.u.intern_type(Type::Applied(function, vec![int, seq, quotes, result]));
    let meth = l.ident(anon, Span::NONE);
    let env = l.tpd.list(&[]);
    let closure = l.tpd.node(Tree::Closure { env, meth, tpt: None }, Span::NONE, Some(function), None);
    let block = l.block(&[def], closure, Span::NONE);
    l.positioned(block)
}

/// `content.apply(quotes.asInstanceOf[Quotes])`, `content` a context function of `Quotes`; the
/// reference to `quotes` at `at`, the span the first reference got, as dotty shares one.
fn applied(l: &mut Lower, content: TNodeId, quotes: SymbolId, quotes_type: TypeId, at: &mut Span) -> TNodeId {
    let tpe = l.u.widen(l.tpd.tpe(content));
    let class = l.u.class_symbol(tpe).expect("a context function");
    let apply = l.u.member(class, "apply")[0];
    let result = l.u.applied(tpe).and_then(|(_, args)| args.last().copied()).expect("a function's result");
    let fun = l.select(content, apply, Span::NONE);
    let fun = l.positioned(fun);
    let quotes = l.ident(quotes, *at);
    let cast_fun = l.select(quotes, l.defs.as_instance_of, Span::NONE);
    let cast_fun = l.positioned(cast_fun);
    let tpt = l.type_tree(quotes_type, Span::NONE);
    let targs = l.tpd.list(&[tpt]);
    let cast = Tree::TypeApply { fun: cast_fun, args: targs };
    let cast = l.tpd.node(cast, Span::NONE, Some(quotes_type), Some(l.defs.as_instance_of));
    let cast = l.positioned(cast);
    let call = l.apply(fun, &[cast], result, Span::NONE);
    let call = l.positioned(call);
    *at = l.tpd.span(quotes);
    call
}

/// Makes `anon` the owner of the definitions under `t` that no other one of them owns.
fn reown(l: &mut Lower, t: TNodeId, anon: SymbolId) {
    let defined: Vec<SymbolId> = subtrees(l.tpd, t)
        .into_iter()
        .filter(|&s| {
            matches!(
                l.tpd.tree(s),
                Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Bind { .. }
            )
        })
        .filter_map(|s| l.tpd.sym(s))
        .collect();
    for &sym in &defined {
        if l.u.symbol(sym).owner.is_none_or(|o| !defined.contains(&o)) {
            l.u.set_owner(sym, anon);
        }
    }
}
