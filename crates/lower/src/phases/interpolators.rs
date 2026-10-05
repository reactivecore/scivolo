//! `s` and `raw` interpolations of literal parts (dotty's `StringInterpolatorOpt`):
//! `StringContext(p0, p1, …).s(a1, …)` becomes `p0 + a1 + p1 + …`, empty parts dropped and the
//! parts of `s` with their escapes processed.

use super::walk::children;
use crate::{Lower, Result, unsupported};
use syntax::ast::constant::Constant;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the interpolations in class definition `td`; fails for one that is not `s` or `raw`
/// of literal parts with valid escapes.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        if let Some(raw) = interpolator(l, t) {
            let span = l.tpd.span(t);
            let literals = |l: &Lower, parts: &[TNodeId]| {
                parts.iter().all(|&p| matches!(l.tpd.tree(p), Tree::Literal { value: Constant::String(_) }))
            };
            let Some((parts, args)) = interpolation(l, t).filter(|(parts, _)| literals(l, parts)) else {
                return unsupported(span, "interpolations of parts that are not literals");
            };
            let Some(texts) = parts.iter().map(|&p| text(l, p, raw)).collect::<Option<Vec<_>>>() else {
                return unsupported(span, "interpolations with invalid escapes");
            };
            for (&p, text) in parts.iter().zip(texts) {
                l.tpd.get_mut(p).tree = Tree::Literal { value: Constant::String(text.into()) };
            }
            let concat = concat(l, &parts, &args);
            let (tree, span, tpe) = (l.tpd.tree(concat).clone(), l.tpd.span(concat), l.tpd.tpe(concat));
            let node = l.tpd.get_mut(t);
            (node.tree, node.span, node.tpe, node.sym) = (tree, span, Some(tpe), None);
        }
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t));
        }
    }
    Ok(())
}

/// Whether `t` is a call of `StringContext`'s `raw`, `Some(false)` for its `s`.
fn interpolator(l: &Lower, t: TNodeId) -> Option<bool> {
    let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { return None };
    let method = l.u.symbol(l.tpd.sym(fun)?);
    if method.owner.map(|o| l.u.full_name(o)).as_deref() != Some("scala.StringContext") {
        return None;
    }
    match method.name.as_str() {
        "s" => Some(false),
        "raw" => Some(true),
        _ => None,
    }
}

/// The parts and arguments of interpolation `StringContext(parts*).s(args*)` at `t`.
fn interpolation(l: &Lower, t: TNodeId) -> Option<(Vec<TNodeId>, Vec<TNodeId>)> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let Tree::Apply { fun: apply, args: parts } = *l.tpd.tree(qualifier) else { return None };
    let apply = l.u.symbol(l.tpd.sym(apply)?);
    if apply.name != "apply"
        || apply.owner.map(|o| l.u.full_name(o)).as_deref() != Some("scala.StringContext$")
    {
        return None;
    }
    let (parts, args) = (sequence(l, parts)?, sequence(l, args)?);
    (parts.len() == args.len() + 1).then_some((parts, args))
}

/// The elements of the single `seq: _*` argument in `args`.
fn sequence(l: &Lower, args: typer::tpd::List) -> Option<Vec<TNodeId>> {
    let &[arg] = l.tpd.items(args) else { return None };
    let seq = match *l.tpd.tree(arg) {
        Tree::Typed { expr, .. } => expr,
        _ => arg,
    };
    let Tree::SeqLiteral { elems, .. } = *l.tpd.tree(seq) else { return None };
    Some(l.tpd.items(elems).to_vec())
}

/// The text of part `p`, its escapes processed unless `raw`; `None` unless a string literal with
/// valid escapes.
fn text(l: &Lower, p: TNodeId, raw: bool) -> Option<Vec<u16>> {
    let Tree::Literal { value: Constant::String(s) } = l.tpd.tree(p) else { return None };
    if raw { Some(s.to_vec()) } else { process_escapes(s) }
}

/// `s` with its escapes replaced (`StringContext.processEscapes`): `\b \t \n \f \r \" \' \\` and
/// `\uXXXX`; `None` for another escape.
fn process_escapes(s: &[u16]) -> Option<Vec<u16>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] != u16::from(b'\\') {
            out.push(s[i]);
            i += 1;
            continue;
        }
        let c = char::from_u32(u32::from(*s.get(i + 1)?))?;
        let replaced = match c {
            'b' => 8,
            't' => 9,
            'n' => 10,
            'f' => 12,
            'r' => 13,
            '"' | '\'' | '\\' => c as u16,
            'u' => {
                let mut j = i + 1;
                while s.get(j) == Some(&u16::from(b'u')) {
                    j += 1;
                }
                let hex = String::from_utf16(s.get(j..j + 4)?).ok()?;
                if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    return None;
                }
                let code = u16::from_str_radix(&hex, 16).ok()?;
                out.push(code);
                i = j + 4;
                continue;
            }
            _ => return None,
        };
        out.push(replaced);
        i += 2;
    }
    Some(out)
}

/// `parts[0] + args[0] + parts[1] + …`, empty parts but the first dropped, each `+` at the span
/// of what it appends.
fn concat(l: &mut Lower, parts: &[TNodeId], args: &[TNodeId]) -> TNodeId {
    let plus = l.u.decl(l.defs.strings.string, "+").unwrap_or_default()[0];
    let tpe = l.u.class_ref("java.lang.String");
    let mut result = parts[0];
    for (&arg, &part) in args.iter().zip(&parts[1..]) {
        result = append(l, result, plus, arg, tpe);
        let empty = matches!(l.tpd.tree(part), Tree::Literal { value: Constant::String(s) } if s.is_empty());
        if !empty {
            result = append(l, result, plus, part, tpe);
        }
    }
    result
}

/// `left + right` at the span of `right`.
fn append(l: &mut Lower, left: TNodeId, plus: SymbolId, right: TNodeId, tpe: TypeId) -> TNodeId {
    let span = l.tpd.span(right);
    let fun = l.select(left, plus, span);
    l.apply(fun, &[right], tpe, span)
}
