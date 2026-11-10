// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//! A quote's tags as type holes (dotty's `encodeTypeArgs`): `t_i.Underlying` becomes a local
//! type `@SplicedType type T_i = {{ i }}` defined before the body.

use super::avoid::avoided;
use super::heal::UNDERLYING;
use crate::definitions::symbol;
use crate::phases::lift_names::fresh;
use crate::phases::walk::subtrees;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::{Span, TName};
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};
use typer::unpickle::QuoteHole;

/// The quote's body with its tags `used` encoded, the type holes, and the references to the
/// tags; `tags` gives the type each tag is the `Type` of, `owner` owns the local types.
pub fn encode(
    l: &mut Lower,
    body: TNodeId,
    (tags, used): (&[(SymbolId, TypeId)], &[TypeId]),
    owner: SymbolId,
) -> Result<(TNodeId, Vec<QuoteHole>, Vec<TNodeId>)> {
    if used.is_empty() {
        return Ok((body, Vec::new(), Vec::new()));
    }
    let span = l.tpd.span(body);
    let at = Span::synthetic(span.end(), span.end());
    let (mut defs, mut holes, mut refs, mut locals) =
        (Vec::new(), Vec::new(), Vec::new(), FxHashMap::default());
    for (index, &tag) in used.iter().enumerate() {
        let Some(evidence) = l.u.sym_of(tag) else { unreachable!("a tag") };
        refs.push(tag_ref(l, tag, at).or_else(|()| unsupported(span, TAG_PATH))?);
        let tpe = tags.iter().find(|(_, t)| *t == tag).map(|&(s, _)| s).expect("a tag's type");
        let name = fresh(l, &l.u.symbol(evidence).name.clone());
        let local = l.u.define(symbol(&name, Kind::Type, owner, Flags::default().with(SYNTHETIC)));
        let underlying = l.u.intern_type(Type::TypeRef(tag, UNDERLYING.into()));
        let alias = l.u.intern_type(Type::Alias(underlying));
        l.u.set_info(local, alias);
        let bound = l.u.intern_type(Type::Sym(None, tpe));
        let bound = avoided(l.u, bound);
        let hole = l.tpd.node(Tree::TypeTree { inferred: false }, at, Some(bound), None);
        holes.push(QuoteHole { node: hole, index: index as u32, args: Vec::new() });
        let name = TName { name: l.names.intern(&name), is_type: true };
        let local_ref = l.u.intern_type(Type::Sym(None, local));
        defs.push(l.tpd.node(Tree::TypeDef { name, rhs: hole }, at, Some(local_ref), Some(local)));
        let class = l.u.class_ref("scala.quoted.runtime.SplicedType");
        let class = l.u.class_symbol(class).expect("SplicedType");
        annotate_new(l, local, class, at);
        locals.insert(underlying, local_ref);
    }
    substitute(l, body, &locals);
    let tpe = l.tpd.tpe(body);
    let block = l.block(&defs, body, span.to_synthetic());
    l.tpd.get_mut(block).tpe = Some(tpe);
    Ok((block, holes, refs))
}

/// The refusal of a tag that is not a local value or a field.
pub const TAG_PATH: &str = "quotes whose type tag is not a local value or field";

/// A reference at `at` to tag `tag`, a local value or a field of an enclosing class.
pub fn tag_ref(l: &mut Lower, tag: TypeId, at: Span) -> std::result::Result<TNodeId, ()> {
    match l.u.sym_ref(tag) {
        Some((None, evidence)) => Ok(l.ident(evidence, at)),
        Some((Some(p), evidence)) if let Some(c) = l.u.this_of(p) => {
            let class = l.u.class_symbol(c).ok_or(())?;
            let this = l.this(class, at);
            Ok(l.select(this, evidence, at))
        }
        _ => Err(()),
    }
}

/// Annotates `local` with `new C()` at `at`, `C` class `class`.
pub fn annotate_new(l: &mut Lower, local: SymbolId, class: SymbolId, at: Span) {
    let init = l.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).expect("a constructor");
    let tpe = l.u.intern_type(Type::Sym(None, class));
    let tpt = l.type_tree(tpe, at);
    let new = l.tpd.node(Tree::New { tpt }, at, Some(tpe), None);
    let fun = l.select(new, init, at);
    let annot = l.apply(fun, &[], tpe, at);
    l.u.annotate(local, class);
    l.tpd.annotate(local, annot);
}

/// Replaces the types `locals` maps by theirs in the types of the trees under `body` and the
/// infos of their definitions.
pub fn substitute(l: &mut Lower, body: TNodeId, locals: &FxHashMap<TypeId, TypeId>) {
    let map = |l: &mut Lower, t: TypeId| l.u.map_type(t, &mut |_, x| locals.get(&x).copied());
    for t in subtrees(l.tpd, body) {
        if let Some(tpe) = l.tpd.get(t).tpe {
            let mapped = map(l, tpe);
            l.tpd.get_mut(t).tpe = Some(mapped);
        }
        let defines = matches!(l.tpd.tree(t), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::Bind { .. });
        if let Some(s) = l.tpd.sym(t).filter(|_| defines) {
            let info = l.u.info(s);
            let mapped = map(l, info);
            l.u.set_info(s, mapped);
        }
    }
}
