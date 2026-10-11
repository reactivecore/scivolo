//! dotty's `Mixin`: a trait's constructor becomes `$init$`, its values deferred accessors
//! (`trait_fields.rs`); a class gets the members its traits' values need and, for each concrete
//! method it inherits from a trait of the unit or the library, a forwarder calling that method
//! on `super`.

use super::trait_fields::{mixed_in, trait_defs};
use crate::definitions::specialized_applies;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, INLINE, PRIVATE};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Whether member `s` has a body: in its TASTy for the library's, else unless deferred.
pub fn has_body(l: &mut Lower, s: SymbolId) -> bool {
    match l.u.symbol(s).tree.is_some() {
        true => l.u.body(s).is_some(),
        false => !l.u.is_deferred(s),
    }
}

/// The methods of the unit's classes that have a body if `with_body`, else those without.
pub fn methods_with_body(l: &Lower, classes: &[TNodeId], with_body: bool) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
        for &stat in l.tpd.items(body) {
            if let (Tree::DefDef { rhs, .. }, Some(s)) = (l.tpd.tree(stat), l.tpd.sym(stat))
                && rhs.is_some() == with_body
            {
                out.insert(s);
            }
        }
    }
    out
}

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let span = l.tpd.span(td);
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    if l.u.is_trait(class) {
        let Tree::DefDef { paramss, .. } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
        let clauses = l.tpd.clause_items(paramss).to_vec();
        let value_params = clauses
            .iter()
            .flat_map(|&c| l.tpd.items(c))
            .any(|&p| !matches!(l.tpd.tree(p), Tree::TypeDef { .. }));
        if value_params {
            return unsupported(span, "trait parameters");
        }
        super::accessor_names::expand_private(l, class);
        let stats = trait_defs(l, td);
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
        return Ok(());
    }
    let mut forwarders = Vec::new();
    for mixin in mixins(l, class) {
        let library = l.u.symbol(mixin).tree.is_some();
        if let Some(arity) = function_arity(l, mixin)
            && specialized_applies(l.u, arity).is_err()
        {
            return unsupported(span, "specialized functions");
        }
        if library && !l.u.no_inits(mixin) {
            super::library_traits::prepare(l, mixin, span)?;
        }
        if !library || !l.u.no_inits(mixin) {
            forwarders.extend(mixed_in(l, class, mixin, span)?);
        }
        for meth in l.u.decls(mixin).unwrap_or_default() {
            if has_body(l, meth) && !has(l.u, meth, INLINE) && needs_forwarder(l, class, meth) {
                let at = if span.exists() { Span::at(span.point()) } else { span };
                forwarders.push(forwarder(l, class, mixin, meth, at)?);
            }
        }
    }
    if forwarders.is_empty() {
        return Ok(());
    }
    let mut stats = forwarders;
    stats.extend_from_slice(l.tpd.items(body));
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    l.recopy(template);
    Ok(())
}

/// The traits `class` mixes in, from the last one linearized to the first.
pub fn mixins(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let linearization = l.u.linearization(class);
    let mut out: Vec<SymbolId> =
        linearization[1..].iter().copied().take_while(|&b| l.u.is_trait(b)).collect();
    out.reverse();
    out
}

/// N if `class` is `scala.FunctionN` with specialized `apply` methods.
pub fn function_arity(l: &Lower, class: SymbolId) -> Option<usize> {
    match l.u.full_name(class).as_str() {
        "scala.Function0" => Some(0),
        "scala.Function1" => Some(1),
        "scala.Function2" => Some(2),
        _ => None,
    }
}

/// Whether `meth` is a method `class` inherits as it is: the first non-private concrete method
/// or value of its name and erased signature in the linearization of `class`.
fn needs_forwarder(l: &mut Lower, class: SymbolId, meth: SymbolId) -> bool {
    let m = l.u.symbol(meth);
    if m.kind != Kind::Def
        || l.u.is_constructor(meth)
        || has(l.u, meth, PRIVATE)
        || has(l.u, meth, FIELDaccessor)
    {
        return false;
    }
    let name = m.name.clone();
    let signature = l.u.erased_method(meth);
    for base in l.u.linearization(class) {
        for other in l.u.decl(base, &name).unwrap_or_default() {
            let s = l.u.symbol(other);
            let member = matches!(s.kind, Kind::Def | Kind::Val) && !has(l.u, other, PRIVATE);
            let deferred = match s.tree.is_some() {
                true => l.u.body(other).is_none(),
                false => s.kind == Kind::Def && !has(l.u, other, FIELDaccessor) && !has_body(l, other),
            };
            if member && !deferred && l.u.erased_method(other) == signature {
                return other == meth;
            }
        }
    }
    false
}

/// `def m(params) = C.super[T].m(params)` in class `class` for method `meth` of trait `mixin`.
fn forwarder(
    l: &mut Lower,
    class: SymbolId,
    mixin: SymbolId,
    meth: SymbolId,
    span: syntax::ast::Span,
) -> Result<TNodeId> {
    let mut info = l.u.info(meth);
    let jdk = is_jdk(l, mixin);
    if jdk {
        info = with_java_param_names(l, info);
    }
    let flags = l.u.symbol(meth).flags;
    let (sym, def) = l.method_like(class, meth, flags, info, span, |l, args, result| {
        let sup = l.super_mix(class, mixin, span);
        let fun = l.select(sup, meth, span);
        l.apply(fun, args, result, span)
    })?;
    if !jdk {
        l.forwarded.insert(sym, meth);
    }
    Ok(def)
}

/// Whether class `c` is in a JDK package, which dotty reads from Java class files.
pub fn is_jdk(l: &Lower, c: SymbolId) -> bool {
    let name = l.u.full_name(c);
    name.starts_with("java.") || name.starts_with("javax.")
}

/// The base classes of `class` that javalib gives JDK classes as parents and the JDK does not,
/// such as `scala.scalanative.runtime.Throwable`.
pub fn javalib_only_bases(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let jdk_bases: Vec<SymbolId> = l.u.linearization(class).into_iter().filter(|&b| is_jdk(l, b)).collect();
    let mut out = Vec::new();
    for b in jdk_bases {
        for p in l.u.parents(b) {
            let Some(p) = l.u.class_symbol(p) else { continue };
            if l.u.full_name(p).starts_with("scala.scalanative.") && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Method type `info` of a JDK method with its parameters named as dotty names those of Java
/// class files: `x$0`, `x$1`, ...
pub fn with_java_param_names(l: &mut Lower, info: TypeId) -> TypeId {
    match l.u.types().get(info).clone() {
        Type::Method { binder, params, result, implicit, contextual } => {
            let params = params.into_iter().enumerate().map(|(i, (_, t))| (format!("x${i}"), t)).collect();
            l.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
        }
        Type::Poly { binder, params, result } => {
            let result = with_java_param_names(l, result);
            l.u.intern_type(Type::Poly { binder, params, result })
        }
        _ => info,
    }
}
