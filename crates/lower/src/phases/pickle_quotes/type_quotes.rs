// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//! Type quotes `Type.of[T](quotes)`: the tag itself for `Type.of[T]` of a `T` with a tag, else
//! `quotes.asInstanceOf[QuoteUnpickler].unpickleTypeV2[T](pickled, tags)`.

use super::heal::{UNDERLYING, heal};
use super::tags::{TAG_PATH, encode, tag_ref};
use super::{pickled, types, unpickled};
use crate::phases::tree_copy::{Copier, copy};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

const TYPE_OF: &str = "scala.quoted.Type$.of";

/// A type quote: `Type.of[body]`, the type application the typer recorded the tags of, and the
/// `Quotes` it is applied to.
pub struct TypeQuote {
    of: TNodeId,
    body: TNodeId,
    quotes: TNodeId,
}

/// `t` as a type quote applied to its `Quotes`.
pub fn quoted(l: &Lower, t: TNodeId) -> Option<TypeQuote> {
    let Tree::Apply { fun: of, args } = *l.tpd.tree(t) else { return None };
    let Tree::TypeApply { fun, args: targs } = *l.tpd.tree(of) else { return None };
    if l.tpd.sym(fun).is_none_or(|s| l.u.full_name(s) != TYPE_OF) {
        return None;
    }
    match (l.tpd.items(targs), l.tpd.items(args)) {
        (&[body], &[quotes]) => Some(TypeQuote { of, body, quotes }),
        _ => None,
    }
}

/// Makes type quote application `t` in `owner` its tag or the call unpickling it.
pub fn pickle(l: &mut Lower, t: TNodeId, q: TypeQuote, owner: SymbolId) -> Result<()> {
    let span = l.tpd.span(q.body);
    let tpe = l.tpd.tpe(q.body);
    let tags = l.tpd.quote_tags(q.of).to_vec();
    let used = heal(l, q.body, &tags)?;
    if let &[tag] = &used[..]
        && l.tpd.tpe(q.body) == l.u.intern_type(Type::TypeRef(tag, UNDERLYING.into()))
    {
        let tag = tag_ref(l, tag, l.tpd.span(t)).or_else(|()| unsupported(span, TAG_PATH))?;
        l.copy_node(t, tag);
        return Ok(());
    }
    let class = match *l.u.types().get(tpe) {
        Type::Sym(_, c) => Some(c),
        _ => None,
    };
    if class.is_some_and(|c| tasty::PRIMITIVE_VALUE_CLASSES.contains(&l.u.full_name(c).as_str())) {
        let tagged = tagged(l, &q, owner)?;
        let tagged = l.with_span(tagged, span);
        l.copy_node(t, tagged);
        return Ok(());
    }
    let (body, holes, tag_refs) = encode(l, q.body, (&tags, &used), owner)?;
    let pickled = pickled(l, body, &holes, span)?;
    let types = types(l, &tag_refs);
    let call = unpickled(l, q.quotes, ("unpickleTypeV2", "scala.quoted.Type", tpe), &[pickled, types]);
    let call = l.with_span(call, span);
    l.copy_node(t, call);
    Ok(())
}

/// `quotes.reflect.TypeReprMethods.asType(quotes.reflect.TypeRepr.typeConstructorOf(classOf[T]))
/// .asInstanceOf[Type[T]]` of type quote `q` of a primitive type `T` in `owner` (dotty's
/// `taggedType`).
fn tagged(l: &mut Lower, q: &TypeQuote, owner: SymbolId) -> Result<TNodeId> {
    let tpe = l.tpd.tpe(q.body);
    let reflect = |l: &mut Lower| -> Result<TNodeId> {
        let quotes = copy(l, &mut Copier { syms: Default::default(), from: owner, to: owner }, q.quotes)?;
        Ok(member(l, quotes, "reflect"))
    };
    let methods = reflect(l)?;
    let methods = member(l, methods, "TypeReprMethods");
    let as_type = member(l, methods, "asType");
    let repr = reflect(l)?;
    let repr = member(l, repr, "TypeRepr");
    let of = member(l, repr, "typeConstructorOf");
    let predef = l.u.class_ref("scala.Predef$");
    let predef = l.u.class_symbol(predef).expect("Predef");
    let class_of = l.u.decl(predef, "classOf").ok().and_then(|d| d.first().copied()).expect("classOf");
    let class_of = l.ident(class_of, Span::NONE);
    let constant = l.u.intern_type(Type::Constant(tasty::tags::CLASSconst, i64::from(tpe.0)));
    let arg = l.type_tree(tpe, Span::NONE);
    let class_of = l.build().type_apply(class_of, &[arg], constant, Span::NONE);
    let class_of = l.positioned(class_of);
    let repr = call(l, of, class_of);
    let as_type = call(l, as_type, repr);
    let class = l.u.class_ref("scala.quoted.Type");
    let quoted = l.u.intern_type(Type::Applied(class, vec![tpe]));
    Ok(l.ensure_conforms(as_type, quoted))
}

/// `qual.name`, of the member `name` of the class of `qual`'s type.
fn member(l: &mut Lower, qual: TNodeId, name: &str) -> TNodeId {
    let tpe = l.u.widen(l.tpd.tpe(qual));
    let class = l.u.class_symbol(tpe).unwrap_or_else(|| panic!("a class with {name}"));
    let members = l.u.member(class, name);
    let Some(&m) = members.iter().find(|&&m| l.u.symbol(m).kind.is_term()) else {
        panic!("{name} in {}", l.u.full_name(class))
    };
    let select = l.select(qual, m, Span::NONE);
    l.positioned(select)
}

/// `fun(arg)`, of the result type of `fun`.
fn call(l: &mut Lower, fun: TNodeId, arg: TNodeId) -> TNodeId {
    let method = l.u.widen(l.tpd.tpe(fun));
    let Type::Method { result, .. } = *l.u.types().get(method) else { panic!("a method") };
    let call = l.apply(fun, &[arg], result, Span::NONE);
    l.positioned(call)
}
