// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/localopt/StringInterpolatorOpt.scala
//! The `s` and `raw` interpolations `StringInterpolatorOpt` does not make concatenations:
//! `sc.s(args)` becomes `StringContext.standardInterpolator(processEscapes, args, sc.parts)`,
//! `raw` passing `x => x`, `sc` evaluated once (dotty's `transformS`).

use super::eval_once::{eval_once, is_stable_path};
use crate::definitions::{applied, symbol};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Rewrites interpolation `t`, of `raw` if `raw`, in `owner`.
pub fn transform(l: &mut Lower, t: TNodeId, raw: bool, owner: SymbolId) -> Result<()> {
    let span = l.tpd.span(t);
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { unreachable!("an interpolation") };
    let (Tree::Select { qualifier: pre, .. }, &[args]) = (l.tpd.tree(fun), l.tpd.items(args)) else {
        return unsupported(span, "interpolations without a `StringContext` prefix");
    };
    let pre = *pre;
    let module = string_context(l)?;
    let class = l.u.module_class(module).expect("StringContext's class");
    let member = |l: &mut Lower, owner: SymbolId, name: &str| -> Result<SymbolId> {
        match l.u.decl(owner, name).unwrap_or_default().first() {
            Some(&m) => Ok(m),
            None => unsupported(span, format!("StringContext.{name} missing from the library")),
        }
    };
    let process_escapes = member(l, class, "processEscapes")?;
    let standard = member(l, class, "standardInterpolator")?;
    let companion = l.u.companion_class(class).expect("the StringContext class");
    let parts = member(l, companion, "parts")?;
    let process = process(l, owner, process_escapes, module, raw);
    let (sc, ev) = if is_stable_path(l, pre) {
        (pre, None)
    } else {
        let (ev, def) = eval_once(l, pre, owner);
        (l.ident(ev, Span::NONE), Some(def))
    };
    let parts = l.select(sc, parts, Span::NONE);
    let parts = l.positioned(parts);
    let qual = l.ident(module, Span::NONE);
    let fun = l.select(qual, standard, Span::NONE);
    let string = l.u.class_ref("java.lang.String");
    let call = l.apply(fun, &[process, args, parts], string, Span::NONE);
    let mut result = l.positioned(call);
    if let Some(def) = ev {
        result = l.block(&[def], result, Span::NONE);
        result = l.positioned(result);
    }
    l.copy_node(t, result);
    Ok(())
}

/// The module `scala.StringContext`.
fn string_context(l: &mut Lower) -> Result<SymbolId> {
    let scala = l.u.package("scala").expect("package scala");
    let found = l.u.decl(scala, "StringContext").unwrap_or_default();
    match found.into_iter().find(|&s| l.u.symbol(s).kind == Kind::Val) {
        Some(m) => Ok(m),
        None => unsupported(Span::NONE, "scala.StringContext missing from the library"),
    }
}

/// `{ def $anonfun(str: String): String = StringContext.processEscapes(str); closure }`, or the
/// identity of `str` if `raw` (dotty's `tpd.Lambda`).
fn process(
    l: &mut Lower,
    owner: SymbolId,
    process_escapes: SymbolId,
    module: SymbolId,
    raw: bool,
) -> TNodeId {
    let string = l.u.class_ref("java.lang.String");
    let info = l.u.info(process_escapes);
    let Some(m) = l.u.method(info) else { unreachable!("a method") };
    let name = m.params[0].0.clone();
    let flags = Flags::default().with(SYNTHETIC).with(ARTIFACT);
    let anon = l.u.define(symbol("$anonfun", Kind::Def, owner, flags));
    let param = l.u.define(symbol(&name, Kind::Param, anon, Flags::default()));
    l.u.set_info(param, string);
    let binder = l.u.fresh_binder();
    let params = vec![(name, string)];
    let method = Type::Method { binder, params, result: string, implicit: false, contextual: false };
    let method = l.u.intern_type(method);
    l.u.set_info(anon, method);
    let param_def = l.param_def(param, Span::NONE);
    let arg = l.ident(param, Span::NONE);
    let rhs = if raw {
        arg
    } else {
        let qual = l.ident(module, Span::NONE);
        let fun = l.select(qual, process_escapes, Span::NONE);
        l.apply(fun, &[arg], string, Span::NONE)
    };
    let tpt = l.type_tree(string, Span::NONE);
    let def = l.method_def(anon, &[param_def], tpt, rhs, Span::NONE);
    let def = l.positioned(def);
    let function = l.u.class_ref("scala.Function1");
    let function_class = l.u.class_symbol(function).expect("`Function1`");
    let function = applied(l.u, function_class, vec![string, string]);
    let meth = l.ident(anon, Span::NONE);
    let env = l.tpd.list(&[]);
    let closure = l.tpd.node(Tree::Closure { env, meth, tpt: None }, Span::NONE, Some(function), None);
    let block = l.block(&[def], closure, Span::NONE);
    l.positioned(block)
}
