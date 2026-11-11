// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala

use crate::Lower;
use syntax::ast::Span;
use typer::tpd::TNodeId;

/// `expr.toX` for primitive `expr` of erased letter `from`, converted to primitive `to`, if the
/// class of `from` has that method (dotty's `primitiveConversion`).
pub fn primitive_conversion(l: &mut Lower, expr: TNodeId, from: char, to: char) -> Option<TNodeId> {
    let class = l.u.class_ref(&crate::definitions::primitive_class_name(from)?);
    let class = l.u.class_symbol(class)?;
    let target = crate::definitions::primitive_class_name(to)?;
    let simple = target.rsplit('.').next()?;
    let method = l.u.decl(class, &format!("to{simple}")).ok()?.into_iter().next()?;
    if from == to {
        return None;
    }
    let select = l.select(expr, method, Span::NONE);
    Some(l.positioned(select))
}

/// `throw new ClassCastException()` at `expr`, which it drops, with dotty's warning that the
/// conversion from type `from` to primitive `to` fails (dotty's `primitiveConversion` without a
/// `toX` method).
pub fn failed_conversion(l: &mut Lower, expr: TNodeId, from: &str, to: char) -> TNodeId {
    let to = crate::definitions::primitive_class_name(to).unwrap_or_default();
    let to = to.trim_start_matches("scala.");
    l.warnings.warning_unpositioned(format!("conversion from {from} to {to} will always fail at runtime."));
    let throw = class_cast_exception(l, None);
    let span = l.tpd.span(expr);
    l.with_span(throw, span)
}

/// `throw new ClassCastException(message)`, or without an argument.
pub fn class_cast_exception(l: &mut Lower, message: Option<&str>) -> TNodeId {
    let class = l.u.class_ref("java.lang.ClassCastException");
    let class = l.u.class_symbol(class).expect("`ClassCastException`");
    let string = l.u.class_ref(tasty::defn::STRING);
    let string = l.u.class_symbol(string);
    let ctors = l.u.decls_named(class, "<init>");
    let ctor = ctors.into_iter().find(|&c| {
        let info = l.u.info(c);
        match l.u.method(info).map(|m| m.params) {
            Some(&[(_, p)]) => message.is_some() && l.u.class_symbol(p) == string,
            Some([]) => message.is_none(),
            _ => false,
        }
    });
    let ctor = ctor.expect("a `ClassCastException` constructor");
    let new = l.new_(class, Span::NONE);
    let args: Vec<TNodeId> = message.into_iter().map(|m| l.string_literal(m)).collect();
    let error = l.call(new, ctor, &args);
    l.tpd.get_mut(error).tpe = l.tpd.get(new).tpe;
    l.throw_(error)
}
