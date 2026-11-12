// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! Implicit members of classes and imports, and references to them.

use super::super::Typer;
use super::super::did_you_mean::is_jdk;
use super::super::paths::MAX_PATH_DEPTH;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use names::nme;
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};

/// An implicit value or method and the prefix it is referenced through.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Candidate {
    pub sym: SymbolId,
    pub prefix: Option<TypeId>,
}

/// The implicit members of class `c` and its base classes, the private ones of `c` if
/// `own_private`, those overridden in a class before them in the linearization left out.
pub fn implicit_members(t: &mut Typer<'_>, c: SymbolId, own_private: bool) -> Vec<SymbolId> {
    let lin = t.u.linearization(c);
    implicit_members_of(t, c, lin, own_private)
}

/// The implicit members of `this` in class `c`: `implicit_members` with the private ones of
/// `c`, then those of the classes of its given self type.
pub fn this_implicit_members(t: &mut Typer<'_>, c: SymbolId) -> Vec<SymbolId> {
    t.complete(c);
    let mut lin = t.u.linearization(c);
    for b in t.u.self_type_bases(c) {
        if !lin.contains(&b) {
            lin.push(b);
        }
    }
    implicit_members_of(t, c, lin, true)
}

fn implicit_members_of(
    t: &mut Typer<'_>,
    c: SymbolId,
    lin: Vec<SymbolId>,
    own_private: bool,
) -> Vec<SymbolId> {
    t.complete(c);
    let site = crate::defn::type_ref(t.u, c);
    let mut out = Vec::new();
    for (i, &base) in lin.iter().enumerate() {
        t.complete(base);
        for d in t.u.decls(base).unwrap_or_default() {
            let s = t.u.symbol(d);
            if s.is_implicit()
                && (own_private && base == c || !s.is_private())
                && !out.contains(&d)
                && !overridden_in(t, site, &lin[..i], d)
            {
                out.push(d);
            }
        }
    }
    out
}

/// Whether a class of `bases` declares a member that overrides `d` as seen from `site`.
fn overridden_in(t: &mut Typer<'_>, site: TypeId, bases: &[SymbolId], d: SymbolId) -> bool {
    let name = t.u.symbol(d).name.clone();
    for &b in bases {
        for m in t.u.decls_named(b, &name) {
            let s = t.u.symbol(m);
            if m != d && s.is_term_member() && !s.is_private() && t.same_member(site, m, d) {
                return true;
            }
        }
    }
    false
}

/// The implicits import `i` makes visible, with the names they are visible under: implicit
/// members by a wildcard, both by `given`, either by name; a wildcard import drops the names
/// it renames.
pub fn imported_implicits(t: &mut Typer<'_>, i: usize) -> Result<Vec<(Candidate, String)>, &'static str> {
    let Some(info) = t.import_info(i) else { return Ok(Vec::new()) };
    let qual = info.qual;
    let names: Vec<(String, String)> =
        info.selectors.iter().map(|s| (s.name.clone(), s.rename.clone())).collect();
    let Some(site) = t.import_site(qual) else { return Ok(Vec::new()) };
    let class_of_value = |t: &mut Typer<'_>| {
        let widened = t.u.widen(qual);
        t.u.class_symbol(widened)
    };
    let classes = match t
        .u
        .module_class(site)
        .or_else(|| t.u.symbol(site).is_class().then_some(site))
        .or_else(|| class_of_value(t))
    {
        Some(class) => vec![class],
        None if t.u.symbol(site).is_package() => {
            t.u.package_objects(site).map_err(|_| "implicit import from an unreadable package object")?
        }
        None if t.u.symbol(site).is_term() => {
            return Err("implicit import from a value of no class type");
        }
        None => return Ok(Vec::new()),
    };
    for &class in &classes {
        for (name, _) in names.iter().filter(|(n, _)| !n.is_empty() && n != nme::WILDCARD) {
            for m in t.class_members(class, name) {
                if matches!(t.syms.completions.get(&m), Some(crate::symbols::Completion::Completing(_))) {
                    return Err("implicit search through a definition being completed");
                }
            }
        }
    }
    let wildcard = names.iter().any(|(n, _)| n == nme::WILDCARD);
    let givens = names.iter().any(|(n, _)| n.is_empty());
    let mut out = Vec::new();
    let members: Vec<_> = classes.into_iter().flat_map(|c| implicit_members(t, c, true)).collect();
    for sym in members {
        let name = t.u.symbol(sym).name.clone();
        let given = t.u.symbol(sym).is_given();
        let excluded = (wildcard || givens) && names.iter().any(|(n, r)| *n == name && *r != name);
        let named = names.iter().find(|(n, r)| *n == name && r != nme::WILDCARD).map(|(_, r)| r.clone());
        let wild = if givens { t.imports_given(i, sym) } else { !given && wildcard };
        if !excluded && (named.is_some() || wild) {
            out.push((Candidate { sym, prefix: Some(qual) }, named.unwrap_or(name)));
        }
    }
    Ok(out)
}

/// A reference to candidate `c` at `span` from `ctx` (dotty's `tpd.ref`): an identifier, or a
/// selection from `this`; a misfit if `c` is inaccessible and not javalib's, or javalib's and
/// would need an inline accessor.
pub fn candidate_ref(t: &mut Typer<'_>, c: Candidate, span: Span, ctx: Ctx) -> Result<TNodeId, &'static str> {
    let javalib = !t.syms.is_script(c.sym) && in_jdk(t.u, c.sym);
    let accessible = t.is_accessible_from(c.sym, c.prefix, ctx);
    if !javalib && !accessible {
        return Err("inaccessible implicit");
    }
    if t.needs_inline_accessor(c.sym, ctx) {
        return Err(if accessible {
            "non-public implicit in an inline method"
        } else {
            "inaccessible implicit"
        });
    }
    let package_object = t.u.symbol(c.sym).owner.filter(|&o| crate::defn::is_named_package_object(t.u, o));
    let through_object = |t: &mut Typer<'_>, p: Option<Type>, o: SymbolId| match p {
        Some(Type::Package(_)) => true,
        Some(Type::This(c)) => match *t.u.ty(c) {
            Type::Package(_) => true,
            Type::Sym(_, s) => s == o || t.u.symbol(s).is_package(),
            _ => false,
        },
        _ => false,
    };
    let prefix = c.prefix.map(|p| *t.u.ty(p));
    let owner = t.u.symbol(c.sym).owner.filter(|&o| t.u.is_class(o));
    if let (Some(Type::Package(p)), Some(owner)) = (&prefix, owner)
        && crate::defn::inheriting_package_object(t.u, *p, owner).is_some()
    {
        return Err("implicit a package object inherits, through its package");
    }
    let tpe = match (prefix, package_object) {
        (_, Some(o)) if through_object(t, prefix, o) => {
            let class = crate::defn::type_ref(t.u, o);
            let this = t.u.intern_type(Type::This(class));
            t.u.intern_type(Type::Sym(Some(this), c.sym))
        }
        _ => t.types().ref_type(c.prefix, c.sym),
    };
    let through_path = !t.term_prefix_is_elidable(c.prefix, c.sym, ctx);
    if through_path && c.prefix.and_then(|p| t.path_tree_from(p, span, MAX_PATH_DEPTH, Some(ctx))).is_none() {
        return Err("implicit through a path");
    }
    Ok(t.ref_(c.prefix, c.sym, tpe, span, ctx))
}

/// Tells whether `s` is in a root package of the JDK, as `is_jdk` of its full name.
fn in_jdk(u: &tasty::Universe, mut s: SymbolId) -> bool {
    while let Some(o) = u.symbol(s).owner.filter(|&o| o != u.root()) {
        s = o;
    }
    is_jdk(&u.symbol(s).name)
}
