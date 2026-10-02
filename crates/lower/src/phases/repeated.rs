//! Repeated arguments (dotty's `ElimRepeated` and `SeqLiterals`): the `SeqLiteral` passed to a
//! Java varargs parameter becomes an array, one passed to a Scala repeated parameter an array
//! wrapped by `ScalaRunTime.wrapXArray`.

use super::walk::children;
use crate::{Lower, Result, unsupported};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The classes whose arrays `ScalaRunTime` wraps by their own method.
const PRIMITIVES: [&str; 9] = ["Boolean", "Byte", "Char", "Short", "Int", "Long", "Float", "Double", "Unit"];

/// Rewrites the repeated arguments of the calls in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        if let Tree::Apply { fun, args } = *l.tpd.tree(t) {
            let mut new_args = Vec::new();
            for (i, &a) in l.tpd.items(args).to_vec().iter().enumerate() {
                new_args.push(repeated_arg(l, fun, i, a)?);
            }
            let args = l.tpd.list(&new_args);
            l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
        }
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t));
        }
    }
    Ok(())
}

/// Argument `a`, the `i`th of a call of `fun`, rewritten if it is `seq: <repeated>[T]`.
fn repeated_arg(l: &mut Lower, fun: TNodeId, i: usize, a: TNodeId) -> Result<TNodeId> {
    let Tree::Typed { expr: seq, tpt } = *l.tpd.tree(a) else { return Ok(a) };
    let Tree::SeqLiteral { elemtpt, .. } = *l.tpd.tree(seq) else { return Ok(a) };
    let ascribed = l.tpd.tpe(tpt);
    if !is_class(l, ascribed, "scala.<repeated>") {
        return Ok(a);
    }
    let span = l.tpd.span(a);
    let elem = l.tpd.tpe(elemtpt);
    let array = l.u.class_ref("scala.Array");
    let array = l.u.intern_type(Type::Applied(array, vec![elem]));
    l.tpd.get_mut(seq).tpe = Some(array);
    let Some(method) = l.tpd.sym(fun) else {
        return unsupported(span, "repeated arguments of a call without a symbol");
    };
    if is_java_varargs(l, method, i) {
        return Ok(seq);
    }
    let Some(wrap) = wrapper(l, elem) else {
        return unsupported(span, "repeated arguments of a generic element type");
    };
    let module = l.defs.scala_run_time;
    let class = l.u.module_class(module).expect("ScalaRunTime's class");
    let Some(&wrap) = l.u.decl(class, &wrap).unwrap_or_default().first() else {
        return unsupported(span, format!("ScalaRunTime.{wrap} missing from the library"));
    };
    let qual = l.ident(module, span);
    let fun = l.select(qual, wrap, span);
    Ok(l.apply(fun, &[seq], ascribed, span))
}

/// Whether the `i`th parameter of `method` is Java varargs: an array marked repeated.
fn is_java_varargs(l: &mut Lower, method: SymbolId, i: usize) -> bool {
    let mut t = l.u.info(method);
    loop {
        match l.u.types().get(t).clone() {
            Type::Poly { result, .. } => t = result,
            Type::Method { params, .. } => {
                return params.get(i).is_some_and(|&(_, p)| match l.u.types().get(p).clone() {
                    Type::Annotated(inner, _) => matches!(l.u.types().get(inner).clone(),
                        Type::Applied(c, _) if is_class(l, c, "scala.Array")),
                    _ => false,
                });
            }
            _ => return false,
        }
    }
}

/// The `ScalaRunTime` method wrapping an array of `elem` (dotty's `wrapArrayMethodName`); `None`
/// for a generic element.
fn wrapper(l: &mut Lower, elem: TypeId) -> Option<String> {
    let class = l.u.class_symbol(elem)?;
    if l.u.symbol(class).kind != Kind::Class {
        return None;
    }
    let name = l.u.full_name(class);
    if let Some(p) = name.strip_prefix("scala.").filter(|p| PRIMITIVES.contains(p)) {
        return Some(format!("wrap{p}Array"));
    }
    let not_runtime =
        ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.Nothing", "scala.Null", "scala.Singleton"];
    (!not_runtime.contains(&name.as_str())).then(|| "wrapRefArray".to_owned())
}

fn is_class(l: &mut Lower, t: TypeId, full: &str) -> bool {
    l.u.class_symbol(t).is_some_and(|c| l.u.full_name(c) == full)
}
