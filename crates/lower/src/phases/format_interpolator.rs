//! `f` interpolations (dotty's `StringInterpolatorOpt.transformF`): `StringContext(parts*).f(args*)`
//! becomes `new StringOps(format).format(args*)`, `format` the checked format string of the parts.

use super::format_checker::{Target, checked};
use super::interpolators::interpolation;
use crate::definitions::type_ref;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

/// Rewrites `f` interpolation `t`; fails for parts or arguments dotty's check rejects or that need
/// a conversion.
pub fn transform(l: &mut Lower, t: TNodeId) -> Result<()> {
    let span = l.tpd.span(t);
    let Some((parts, args, seq)) = interpolation(l, t) else {
        return unsupported(span, "f interpolations without a statically known `StringContext`");
    };
    let texts: Option<Vec<Vec<u16>>> = parts
        .iter()
        .map(|&p| match l.tpd.tree(p) {
            Tree::Literal { value: Constant::String(s) } => Some(s.to_vec()),
            _ => None,
        })
        .collect();
    let Some(texts) = texts else { return unsupported(span, "f interpolations of non-literal parts") };
    let texts: Vec<&[u16]> = texts.iter().map(Vec::as_slice).collect();
    let types: Vec<_> = args.iter().map(|&a| l.tpd.tpe(a)).collect();
    let mut conforms = |i: usize, target: Target| {
        let class = match target {
            Target::Any => return true,
            Target::Boolean => "scala.Boolean",
            Target::Null => "scala.Null",
            Target::Char => "scala.Char",
            Target::Byte => "scala.Byte",
            Target::Short => "scala.Short",
            Target::Int => "scala.Int",
            Target::Long => "scala.Long",
            Target::Float => "scala.Float",
            Target::Double => "scala.Double",
            Target::BigInt => "scala.math.BigInt",
            Target::BigDecimal => "scala.math.BigDecimal",
            Target::Calendar => "java.util.Calendar",
            Target::Date => "java.util.Date",
            Target::Formattable => "java.util.Formattable",
        };
        let target = l.u.class_ref(class);
        l.u.is_subtype(types[i], target)
    };
    let format = match checked(&texts, &mut conforms) {
        Ok(format) => format,
        Err(why) => return unsupported(span, why),
    };
    let string_ops = l.u.class_ref("scala.collection.StringOps");
    let class = l.u.class_symbol(string_ops).expect("`StringOps`");
    let member = |l: &mut Lower, name: &str| *l.u.decl(class, name).unwrap_or_default().first().expect(name);
    let (init, format_method) = (member(l, "<init>"), member(l, "format"));
    let value = Constant::String(format.into());
    let tpe = typer::tpd::constant_type_of(l.u, &value);
    let literal = l.tpd.node(Tree::Literal { value }, Span::NONE, Some(tpe), None);
    let new = l.new_(class, Span::NONE);
    let ctor = l.select(new, init, Span::NONE);
    let ops_type = type_ref(l.u, class);
    let ops = l.apply(ctor, &[literal], ops_type, Span::NONE);
    let fun = l.select(ops, format_method, Span::NONE);
    let string = l.u.class_ref("java.lang.String");
    reposition(l, seq);
    let call = l.apply(fun, &[seq], string, Span::NONE);
    let call = l.positioned(call);
    l.copy_node(t, call);
    Ok(())
}

/// Gives argument `seq` and the sequence literal in it the spans of their children, as dotty's
/// new `SeqLiteral` of the format arguments has.
fn reposition(l: &mut Lower, seq: TNodeId) {
    if let Tree::Typed { expr, .. } = *l.tpd.tree(seq) {
        l.tpd.get_mut(expr).span = Span::NONE;
        l.positioned(expr);
    }
    l.tpd.get_mut(seq).span = Span::NONE;
    l.positioned(seq);
}
