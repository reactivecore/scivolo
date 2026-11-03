// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimRepeated.scala
//! dotty's `ElimRepeated.addVarArgsForwarder`: a method with `@varargs` or overriding a Java
//! varargs method gets a forwarder taking an array, which calls it with the array wrapped.

use super::array_wraps::{array_of, wrap_array};
use super::repeated::{is_class, java_array};
use crate::definitions::symbol;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, OVERRIDE, PRIVATE};
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Adds the forwarders to class definition `td`, each after its method.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let stats = l.tpd.items(body).to_vec();
    let mut new = Vec::with_capacity(stats.len());
    for stat in stats {
        new.push(stat);
        let Tree::DefDef { .. } = l.tpd.tree(stat) else { continue };
        let Some(meth) = l.tpd.sym(stat) else { continue };
        let annotated = has_varargs(l, meth);
        if !has_repeated(l, meth) {
            if annotated {
                return unsupported(l.tpd.span(stat), "@varargs on a method without repeated parameters");
            }
            continue;
        }
        let overridden = overridden(l, meth);
        let java = overridden.iter().any(|&m| java_varargs(l, m));
        let inherited = overridden.iter().any(|&m| has_varargs(l, m));
        let flags = l.u.symbol(meth).flags;
        let flags = match (java, annotated && !inherited) {
            (true, _) => flags.with(ARTIFACT),
            (false, true) => flags.without(OVERRIDE),
            (false, false) => flags,
        };
        if annotated || java || inherited {
            new.push(forwarder(l, class, (meth, flags), stat, java)?);
        }
    }
    let body = l.tpd.list(&new);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `def meth(…, xs: Array[T]) = this.meth(…, wrapXArray(xs))` for `meth(…, xs: T*)` in `class`,
/// with `flags`; `java` if `meth` overrides Java varargs.
fn forwarder(
    l: &mut Lower,
    class: SymbolId,
    (meth, flags): (SymbolId, Flags),
    def: TNodeId,
    java: bool,
) -> Result<TNodeId> {
    let span = l.tpd.span(def);
    let info = l.u.info(meth);
    let Type::Method { params, result, implicit, contextual, .. } = l.u.types().get(info).clone() else {
        return unsupported(span, "varargs forwarders of polymorphic or curried methods");
    };
    if !l.u.param_clauses(result).is_empty() {
        return unsupported(span, "varargs forwarders of polymorphic or curried methods");
    }
    let Some(&(_, repeated)) = params.last().filter(|&&(_, p)| is_class(l, p, "scala.<repeated>")) else {
        return unsupported(span, "@varargs on a method whose last parameter is not repeated");
    };
    let Type::Applied(_, ref elems) = *l.u.types().get(repeated) else { unreachable!("`<repeated>[T]`") };
    let elem = elems[0];
    let array = array_of(l, elem);
    if !matches!(l.u.erase(array), Erased::Array(_)) {
        return unsupported(span, "varargs forwarders of generic element types");
    }
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { unreachable!("a method definition") };
    let clause = l.tpd.clause_items(paramss).first().map(|&c| l.tpd.items(c).to_vec()).unwrap_or_default();
    let names = clause.iter().map(|&p| l.tpd.sym(p).map(|s| l.u.symbol(s).name.clone()));
    let mut fparams: Vec<(String, TypeId)> =
        params.iter().zip(names).map(|((n, t), m)| (m.unwrap_or_else(|| n.clone()), *t)).collect();
    fparams.last_mut().expect("a repeated parameter").1 = array;
    let binder = l.u.fresh_binder();
    let finfo = l.u.intern_type(Type::Method { binder, params: fparams, result, implicit, contextual });
    let name = l.u.symbol(meth).name.clone();
    if conflicts(l, class, &name, finfo, java) {
        return unsupported(span, "a varargs forwarder conflicting with a member of its signature");
    }
    let sym = l.u.define(symbol(&name, Kind::Def, class, flags));
    l.u.set_info(sym, finfo);
    l.varargs_forwarders.insert(sym);
    l.coords.insert(sym, Span::NONE);
    let at = Span::at(span.start());
    let def = l.forwarding_def(sym, span.to_synthetic(), |l, args, _| {
        let mut args = args.to_vec();
        let last = args.pop().expect("a repeated parameter");
        let wrapped = wrap_array(l, last, elem, repeated, at);
        args.push(wrapped.expect("ScalaRunTime's wrappers"));
        let this = l.this(class, at);
        let fun = l.select(this, meth, at);
        l.apply(fun, &args, result, at)
    });
    Ok(l.positioned(def))
}

/// Whether `member` overriding `other` is left to `member`'s forwarder: its repeated parameter
/// is Java varargs in `other`.
pub fn forwarded(l: &mut Lower, member: SymbolId, other: SymbolId) -> bool {
    has_repeated(l, member) && java_varargs(l, other)
}

fn params(l: &mut Lower, s: SymbolId) -> Vec<TypeId> {
    let info = l.u.info(s);
    l.u.param_clauses(info).into_iter().flat_map(|(_, ps)| ps).collect()
}

fn has_repeated(l: &mut Lower, s: SymbolId) -> bool {
    params(l, s).into_iter().any(|p| is_class(l, p, "scala.<repeated>"))
}

fn java_varargs(l: &mut Lower, s: SymbolId) -> bool {
    params(l, s).into_iter().any(|p| java_array(l, p).is_some())
}

fn has_varargs(l: &mut Lower, s: SymbolId) -> bool {
    l.u.annotations(s).iter().any(|&c| l.u.full_name(c) == "scala.annotation.varargs")
}

/// The non-private methods of the base classes of `def`'s owner that `def` matches.
fn overridden(l: &mut Lower, def: SymbolId) -> Vec<SymbolId> {
    let Some(owner) = l.u.symbol(def).owner.filter(|&o| l.u.is_class(o)) else { return Vec::new() };
    let name = l.u.symbol(def).name.clone();
    let site = l.u.intern_type(Type::Sym(None, owner));
    let site = l.u.intern_type(Type::This(site));
    let mut found = Vec::new();
    for base in l.u.linearization(owner).into_iter().skip(1) {
        for m in l.u.decl(base, &name).unwrap_or_default() {
            let s = l.u.symbol(m);
            if s.kind == Kind::Def && !s.flags.has(PRIVATE) && l.u.info_matches(site, def, m) {
                found.push(m);
            }
        }
    }
    found
}

/// Whether a member of `class` named `name`, but a varargs forwarder and, for an override of
/// Java varargs, a Java varargs method, has the erasure of method type `info`.
fn conflicts(l: &mut Lower, class: SymbolId, name: &str, info: TypeId, java: bool) -> bool {
    let erased = l.u.erased_method_type(info);
    for base in l.u.linearization(class) {
        for m in l.u.decl(base, name).unwrap_or_default() {
            let s = l.u.symbol(m);
            if s.kind != Kind::Def || (base != class && s.flags.has(PRIVATE)) {
                continue;
            }
            if l.varargs_forwarders.contains(&m) || (java && java_varargs(l, m)) {
                continue;
            }
            if l.u.erased_method(m) == erased {
                return true;
            }
        }
    }
    false
}
