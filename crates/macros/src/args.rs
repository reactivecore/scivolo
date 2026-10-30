// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/Interpreter.scala
//! The arguments of a macro implementation as the VM passes them (dotty's `interpretArgs`).

use crate::host::Host;
use syntax::ast::constant::Constant;
use tasty::{Erased, SymbolId, Type, Universe};
use typer::inlining::MacroArg;

/// An argument as the VM passes it, or a quoted expression or type the host has yet to number.
pub enum Flat<'a> {
    Ready(vm::MacroArg),
    Quoted(&'a MacroArg),
}

/// Argument `a` for a parameter erased to `param`, appended to `out` as the VM passes it: a
/// by-name argument in a `Function0`, the elements of a repeated one in a `Seq`.
pub fn flatten<'a>(a: &'a MacroArg, param: &Erased, out: &mut Vec<Flat<'a>>) -> Result<(), &'static str> {
    let object = Erased::Class("java.lang.Object".into());
    if *param == Erased::Class("scala.Function0".into()) {
        out.push(Flat::Ready(vm::MacroArg::ByName));
        return flatten(a, &object, out);
    }
    match a {
        MacroArg::Expr(_) | MacroArg::Type(_) => out.push(Flat::Quoted(a)),
        MacroArg::Quotes => out.push(Flat::Ready(vm::MacroArg::Quotes)),
        MacroArg::Literal(_) => out.push(Flat::Ready(
            literal(a, param)
                .flatten()
                .ok_or("a literal macro argument of another primitive type than its parameter")?,
        )),
        MacroArg::Varargs(elems) => {
            out.push(Flat::Ready(vm::MacroArg::Varargs(elems.len() as u32)));
            elems.iter().try_for_each(|e| flatten(e, &object, out))?;
        }
    }
    Ok(())
}

/// The arguments `flat`, their quoted expressions and types numbered by `host`.
pub fn numbered(flat: Vec<Flat<'_>>, host: &mut Host) -> Vec<vm::MacroArg> {
    let one = |f| match f {
        Flat::Ready(a) => a,
        Flat::Quoted(&MacroArg::Expr(t)) => vm::MacroArg::Expr(host.add_tree(t)),
        Flat::Quoted(&MacroArg::Type(t)) => vm::MacroArg::Type(host.add_type(t)),
        Flat::Quoted(_) => unreachable!("a quoted expression or type"),
    };
    flat.into_iter().map(one).collect()
}

/// Argument `a` as the VM passes it to a parameter erased to `param` if it is a literal: unboxed
/// to a primitive of its type, else boxed by the descriptor letter of its type (dotty's
/// `interpretLiteral`); `None` for a primitive of another type.
pub fn literal(a: &MacroArg, param: &Erased) -> Option<Option<vm::MacroArg>> {
    let MacroArg::Literal(c) = a else { return Some(None) };
    let (tag, bits, text) = match c {
        Constant::Unit => (b'V', 0, None),
        &Constant::Boolean(b) => (b'Z', b as u64, None),
        &Constant::Byte(n) => (b'B', n as u8 as u64, None),
        &Constant::Short(n) => (b'S', n as u16 as u64, None),
        &Constant::Char(n) => (b'C', n as u64, None),
        &Constant::Int(n) => (b'I', n as u32 as u64, None),
        &Constant::Long(n) => (b'J', n as u64, None),
        &Constant::Float(x) => (b'F', x.to_bits() as u64, None),
        &Constant::Double(x) => (b'D', x.to_bits(), None),
        Constant::String(units) => (b'T', 0, Some(String::from_utf16_lossy(units))),
        Constant::Null => (b'N', 0, None),
    };
    Some(Some(match *param {
        Erased::Prim(p) if p as u8 == tag.to_ascii_lowercase() => vm::MacroArg::Unboxed(bits),
        Erased::Prim(_) => return None,
        _ => vm::MacroArg::Literal(tag, bits, text),
    }))
}

/// Whether macro implementation `m` returns a context function taking the `Quotes` by name.
pub fn quotes_by_name(u: &mut Universe, m: SymbolId) -> bool {
    let mut t = u.info(m);
    while let Type::Method { result, .. } | Type::Poly { result, .. } | Type::ByName(result) =
        *u.types().get(t)
    {
        t = result;
    }
    let t = u.dealias(t);
    let types = u.types();
    matches!(types.get(t), Type::Applied(_, targs)
        if targs.first().is_some_and(|&a| matches!(types.get(a), Type::ByName(_))))
}

/// Signature `sig` of a macro implementation whose result is a context function of the `Quotes`
/// as dotty's erasure gives it: the `Quotes`, a `Function0` of them if `by_name`, its last
/// parameter (`ContextFunctionResults`).
pub fn quotes_last(sig: &str, by_name: bool) -> Option<String> {
    let sig = sig.strip_suffix("L15scala.Function1EO")?;
    let quotes = if by_name { "L15scala.Function0" } else { "L19scala.quoted.Quotes" };
    Some(format!("{sig}{quotes}L17scala.quoted.ExprEO"))
}
