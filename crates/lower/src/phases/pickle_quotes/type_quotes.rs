// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/staging/CrossStageSafety.scala
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//! Type quotes `Type.of[T](quotes)`: the tag itself for `Type.of[T]` of a `T` with a tag, else
//! `quotes.asInstanceOf[QuoteUnpickler].unpickleTypeV2[T](pickled, tags)`.

use super::heal::{UNDERLYING, heal};
use super::reflect::{Reflect, call};
use super::tags::{TAG_PATH, encode, tag_ref};
use super::{pickled, types, unpickled};
use crate::{Lower, Result, unsupported};
use names::nme;
use syntax::ast::Span;
use tasty::defn::TYPE_OF;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// A type quote: `Type.of[body]`, the type application the typer recorded the tags of, and the
/// `Quotes` it is applied to.
pub struct TypeQuote {
    pub of: TNodeId,
    pub body: TNodeId,
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

/// Makes type quote application `t` in `owner` of source file `file` its tag or the call
/// unpickling it.
pub fn pickle(l: &mut Lower, t: TNodeId, q: TypeQuote, owner: SymbolId, file: Option<usize>) -> Result<()> {
    let span = l.tpd.span(q.body);
    let tpe = l.tpd.tpe(q.body);
    let tags = l.tpd.quote_tags(q.of).to_vec();
    let used = heal(l, q.body, &tags)?;
    let underlying = l.u.intern_name(UNDERLYING);
    if let &[tag] = &used[..]
        && !l.spliced_type_quotes.contains(&q.of)
        && l.tpd.tpe(q.body) == l.u.intern_type(Type::TypeRef(tag, underlying))
    {
        let tag = tag_ref(l, tag, l.tpd.span(t)).or_else(|()| unsupported(span, TAG_PATH))?;
        l.copy_node(t, tag);
        return Ok(());
    }
    let class = match *l.u.ty(tpe) {
        Type::Sym(_, c) => Some(c),
        Type::TypeRef(..) => l.u.resolve_ref(tpe),
        _ => None,
    };
    if class.is_some_and(|c| tasty::PRIMITIVE_VALUE_CLASSES.contains(&l.u.full_name(c).as_str())) {
        let tagged = tagged(l, &q, owner)?;
        let tagged = l.with_span(tagged, span);
        l.copy_node(t, tagged);
        return Ok(());
    }
    let (body, holes, tag_refs) = encode(l, q.body, (&tags, &used), owner)?;
    let pickled = pickled(l, body, &holes, span, file)?;
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
    let reflect = Reflect { quotes: q.quotes, owner };
    let as_type = reflect.method(l, "TypeReprMethods", nme::AS_TYPE)?;
    let of = reflect.method(l, "TypeRepr", "typeConstructorOf")?;
    let predef = l.u.class_ref("scala.Predef$");
    let predef = l.u.class_symbol(predef).expect("Predef");
    let class_of = l.u.decl(predef, nme::CLASS_OF).ok().and_then(|d| d.first().copied()).expect("classOf");
    let class_of = l.ident(class_of, Span::NONE);
    let constant = l.u.intern_type(Type::Constant(tasty::tags::CLASSconst, i64::from(tpe.0)));
    let arg = l.type_tree(tpe, Span::NONE);
    let class_of = l.build().type_apply(class_of, &[arg], constant, Span::NONE);
    let class_of = l.positioned(class_of);
    let repr = call(l, of, &[class_of]);
    let as_type = call(l, as_type, &[repr]);
    let class = l.u.class_ref("scala.quoted.Type");
    let quoted = l.u.intern_applied(class, &[tpe]);
    Ok(l.ensure_conforms(as_type, quoted))
}
