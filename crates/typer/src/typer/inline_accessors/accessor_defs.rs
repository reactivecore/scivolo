// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameKinds.scala
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The definitions of inline accessors: their class, name and forwarding body (dotty's
//! `hostForAccessorOf`, `InlineAccessorName`, `accessorDef`).

use super::super::Typer;
use super::Access;
use super::accessor_types::{Clause, accessor_tpt};
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId, Universe, name_kinds, nme};

/// Makes `def inline$m(ps): T = C.this.m(ps)` in class `host` for `access`, or
/// `def inline$m_=(x$0: T): Unit = C.this.m = x$0` for a setter; `None`, reported at `span`, if
/// a type cannot be rebuilt around its parameters or the result is a context function.
pub fn accessor_def(t: &mut Typer<'_>, host: SymbolId, access: Access, span: Span) -> Option<NodeId> {
    let name = accessor_name(t.u, access, host);
    let accessed = access.member;
    t.complete(accessed);
    let mut info = t.u.info(accessed);
    if t.u.is_module(accessed)
        && let Some((prefix, _)) = t.u.sym_ref(info)
    {
        info = t.u.intern_type(Type::Sym(prefix, accessed));
    }
    let mut clauses = if access.setter { Vec::new() } else { method_clauses(t.u, &mut info) };
    if t.is_context_function_type(info) {
        t.unsupported_at(span, "inline accessor of a context function result");
        return None;
    }
    let result = if access.setter {
        let params = vec![(t.u.intern_name(&name_kinds::synthetic_param(0)), info)];
        clauses.push(Clause { binder: None, params, flags: f::TERM_PARAM });
        crate::defn::type_ref(t.u, t.defn.unit)
    } else {
        info
    };

    let at = accessor_span(t, accessed, span);
    let target = accessed_ref(t, host, accessed, at);
    let mut rhs = t.ast.node(Tree::TypeTree);
    t.splices.insert(rhs, target);
    let mut defs = Vec::new();
    for (i, clause) in clauses.iter().enumerate() {
        let is_type = clause.flags.is(f::TYPE_PARAM);
        let mut params = Vec::new();
        let mut args = Vec::new();
        for &(pname, ptype) in &clause.params {
            let pname = t.u.name(pname).to_owned();
            let name = TName { name: t.name(&pname), is_type };
            let tpt = accessor_tpt(t, ptype, &clauses[..=i], span)?;
            let mods = t.ast.mods(Modifiers::with_flags(clause.flags));
            params.push(match is_type {
                true => t.ast.node(Tree::TypeDef { name, rhs: tpt, mods }),
                false => t.ast.node(Tree::ValDef { name, tpt, rhs: None, mods }),
            });
            args.push(t.ast.node(Tree::Ident { name }));
        }
        let fun = rhs;
        let list = t.ast.list(&args);
        rhs = match (is_type, access.setter) {
            (true, _) => t.ast.node(Tree::TypeApply { fun, args: list }),
            (false, true) => t.ast.node(Tree::Assign { lhs: fun, rhs: args[0] }),
            (false, false) => t.ast.node(Tree::Apply { fun, args: list }),
        };
        if clause.flags.is(f::GIVEN) || clause.flags.is(f::IMPLICIT) {
            t.ast.add_attrs(rhs, syntax::ast::Attrs::USING_APPLY);
        }
        defs.push(t.ast.list(&params));
    }

    let tpt = accessor_tpt(t, result, &clauses, span)?;
    let paramss = t.ast.clauses(&defs);
    let private = t.u.symbol(accessed).flags.has(tasty::tags::PRIVATE);
    let flags = f::SYNTHETIC.or(if private { f::FINAL } else { f::EMPTY });
    let mods = t.ast.mods(Modifiers::with_flags(flags));
    let name = TName { name: t.name(&name), is_type: false };
    let def = t.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods });
    t.ast.with_span(def, at);
    Some(def)
}

/// The parameter clauses of method type `info`, which is left at the method's result.
fn method_clauses(u: &Universe, info: &mut TypeId) -> Vec<Clause> {
    let mut clauses = Vec::new();
    while let Type::Poly { binder, params, result: r } | Type::Method { binder, params, result: r, .. } =
        *u.ty(*info)
    {
        let flags = match *u.ty(*info) {
            Type::Poly { .. } => f::TYPE_PARAM,
            Type::Method { contextual: true, .. } => f::TERM_PARAM.or(f::GIVEN),
            Type::Method { implicit: true, .. } => f::TERM_PARAM.or(f::IMPLICIT),
            _ => f::TERM_PARAM,
        };
        clauses.push(Clause { binder: Some(binder), params: u.params_of(params).to_vec(), flags });
        *info = r;
    }
    clauses
}

/// A reference to `accessed` from the `this` of `host`: an identifier for a member of a
/// static object or a parameter accessor outside traits, else a selection (dotty's `ref`).
fn accessed_ref(t: &mut Typer<'_>, host: SymbolId, accessed: SymbolId, at: Span) -> TNodeId {
    let owner = t.u.symbol(accessed).owner.expect("a member");
    if t.u.is_static_owner(owner) {
        return t.member_ref(accessed, at);
    }
    let host_type = crate::defn::type_ref(t.u, host);
    let param = t.syms.flags.get(&accessed).is_some_and(|fl| fl.is_one_of(f::PARAM.or(f::PARAM_ACCESSOR)));
    if param && !t.u.is_trait(host) && !t.u.is_trait(owner) {
        let this = t.u.intern_type(Type::This(host_type));
        let tpe = t.u.intern_type(Type::Sym(Some(this), accessed));
        return t.build().ident(accessed, tpe, at);
    }
    let this = t.build().this(host_type, at);
    t.build().select(this, accessed, at)
}

/// Determines the class that hosts the accessor of `accessed` for a reference from `owner`:
/// the closest class around `owner` that has `accessed` as a member or is the companion of its
/// object, the top-level class around `owner` for a member of its package (dotty's
/// `hostForAccessorOf`); `None` if no class around `owner` sees `accessed`.
pub fn accessor_host(u: &mut Universe, accessed: SymbolId, owner: SymbolId) -> Option<SymbolId> {
    let target = u.symbol(accessed).owner?;
    let mut o = Some(owner);
    let mut top = None;
    while let Some(c) = o {
        if u.symbol(c).kind == Kind::Class {
            let companion = u.companion_module(c).and_then(|m| u.module_class(m));
            if u.derives_from(c, target) || companion == Some(target) {
                return Some(c);
            }
            top = Some(c);
        }
        if u.symbol(c).kind == Kind::Package {
            return (c == target).then_some(top).flatten();
        }
        o = u.symbol(c).owner;
    }
    None
}

/// The name of the accessor of `access` in class `host`: expanded by the class's name if it may
/// have subclasses (dotty's `accessorNameOf`).
fn accessor_name(u: &Universe, access: Access, host: SymbolId) -> String {
    let prefix = u.expanded_prefix(host);
    let base = name_kinds::inline_accessor(&u.symbol(access.member).name);
    let s = u.symbol(host);
    let extensible =
        !s.flags.has(tasty::tags::FINAL) && !u.is_module_class(host) && !s.name.starts_with(nme::ANON_CLASS);
    let name = if extensible { name_kinds::expanded(&prefix, &base) } else { base };
    if access.setter { format!("{name}_=") } else { name }
}

/// Determines where the accessors of `accessed` are defined: at its definition's point, else at
/// the point of `span`, the reference that needs them.
pub fn accessor_span(t: &mut Typer<'_>, accessed: SymbolId, span: Span) -> Span {
    match t.syms.def_trees.get(&accessed) {
        Some(&d) => Span::at(t.ast.span(d).point()),
        None => t.u.pickled_point(accessed).map_or(Span::at(span.point()), |p| Span::at(p).in_other_source()),
    }
}
