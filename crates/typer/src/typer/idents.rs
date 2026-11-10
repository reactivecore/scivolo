// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Identifiers (dotty's `typedIdent`) and the references they make.

use super::overloads::{AMBIGUOUS, NO_APPLICABLE, NO_SIZE, Unchosen};
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

/// What a name refers to: its prefix (`None` for a local) and the symbols it denotes, several
/// for an overloaded method.
pub struct Found {
    pub prefix: Option<TypeId>,
    pub syms: Vec<SymbolId>,
}

impl Typer<'_> {
    pub fn members_found(&mut self, prefix: TypeId, name: &str, is_type: bool) -> Option<Found> {
        let syms: Vec<SymbolId> = self
            .type_members(prefix, name)
            .into_iter()
            .filter(|&s| self.u.is_type_symbol(s) == is_type)
            .collect();
        (!syms.is_empty()).then_some(Found { prefix: Some(prefix), syms })
    }

    pub fn typed_ident(&mut self, id: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let text = self.text(name.name).to_owned();
        if ctx.mode.is(Mode::PATTERN) && text == "_" {
            return self.wildcard_pattern(id, name, pt);
        }
        if ctx.mode.is(Mode::PATTERN) && !name.is_type && self.is_var_pattern(id, &text) {
            return self.pattern_var(id, name, pt, ctx);
        }
        if self.quoted_type_var(id, ctx).is_some() {
            return self.typed_quoted_type_var(id, name, pt);
        }
        if (text == "_root_" || text == "<empty>") && !name.is_type {
            let root = self.u.root();
            let tpe = self.u.intern_type(Type::Package(root));
            let span = self.ast.span(id);
            return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(root));
        }
        let found = match self.find_ref(&text, name.is_type, ctx) {
            Ok(Some(found)) => found,
            Ok(None) if matches!(pt, Pt::Fun(_) | Pt::PolyFun(_)) && !name.is_type => {
                return self.proxy_ident(id, name, pt, ctx);
            }
            Ok(None) => {
                if let Some(t) = self.unresolved_term(id, name, ctx) {
                    return t;
                }
                return match self.not_found(id, name, ctx) {
                    Some(error) => error,
                    None => match self.unbound_in_checked_code(id, name, ctx) {
                        Some(error) => error,
                        None => self.unsupported(id, &format!("unresolved name {text}")),
                    },
                };
            }
            Err("constructor proxy") if matches!(pt, Pt::Fun(_) | Pt::PolyFun(_)) => {
                return self.proxy_ident(id, name, pt, ctx);
            }
            Err(what) => return self.unsupported(id, what),
        };
        if let [s] = found.syms[..]
            && self.u.symbol(s).owner.is_some_and(|c| self.self_named(c, &text) == Some(s))
        {
            return self.self_ref(id, name, s);
        }
        if !name.is_type
            && let Some(sel) = self.local_extension_selection(id, name, &found.syms, pt, ctx)
        {
            return sel;
        }
        let sym = match self.choose(&found.syms, found.prefix, pt) {
            Ok(sym) => sym,
            Err(Unchosen { what: what @ (NO_APPLICABLE | NO_SIZE | AMBIGUOUS), ambiguous })
                if !self.unusable_for_inference(pt) =>
            {
                return match self.overload_error(id, (&found.syms, found.prefix), pt, ambiguous) {
                    Some(error) => error,
                    None => self.unsupported(id, what),
                };
            }
            Err(e) => return self.unchosen(id, e.what, pt),
        };
        if let Err(what) = self
            .check_reference(sym, found.prefix, ctx)
            .and_then(|()| self.check_erased_reference(sym, pt, ctx))
        {
            return self.unsupported(id, what);
        }
        if self.is_staging_method(sym) {
            return self.unsupported(id, super::source_splices::STAGING_REFERENCE);
        }
        if let Err(what) = self.check_value(sym, name, pt, ctx) {
            let Some(object) = self.package_object_value(sym, ctx) else { return self.unsupported(id, what) };
            let qual = self.typed_ident(id, name, Pt::Any, ctx.with_mode(ctx.mode.with(Mode::QUALIFIER)));
            let span = self.ast.span(id);
            return self.build().select(qual, object, span);
        }
        if found.prefix.is_some_and(|p| self.jdk_redeclares(sym, p)) {
            return self.unsupported(id, "Object member a JDK interface redeclares");
        }
        if name.is_type && self.u.full_name(sym).starts_with("scala.Impure") {
            return self.unsupported(id, "impure function type");
        }
        if text == "deferred" && self.u.full_name(sym).starts_with("scala.compiletime.") {
            return self.unsupported(id, "deferred given");
        }
        let tpe = self.ref_type(found.prefix, sym);
        let span = self.ast.span(id);
        let overloaded_accessor = found.syms.len() > 1
            && self.is_param_or_accessor(sym)
            && found.prefix.is_some_and(|p| {
                let class = self.u.this_of(p).and_then(|c| self.u.class_symbol(c));
                class.is_some_and(|c| !self.u.is_static_owner(c))
            });
        if overloaded_accessor || !self.prefix_is_elidable(tpe, ctx) {
            if name.is_type {
                return self.type_tree(tpe, span, false);
            }
            if let Some(this) = self.module_this(sym, span, ctx) {
                return this;
            }
            return self.this_select(span, name, tpe, sym, ctx);
        }
        self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym))
    }

    /// A reference desugaring made to the library symbol of full name `sym`: a type tree for a
    /// type (dotty's `TypeTree(sym.typeRef)`), else an identifier of its simple name (`ref(sym)`).
    pub fn typed_full_ref(&mut self, id: NodeId, sym: TName, ctx: Ctx) -> TNodeId {
        let full = self.text(sym.name).to_owned();
        let span = self.ast.span(id);
        if sym.is_type {
            let Some(tpe) = self.full_type(&full, ctx) else {
                return self.unsupported(id, &format!("reference to {full}"));
            };
            return self.type_tree(tpe, span, false);
        }
        let Some(s) = self.resolve_full_name(&full, false) else {
            return self.unsupported(id, &format!("reference to {full}"));
        };
        self.build().static_ident(s, span)
    }

    /// The symbol of full name `full`, e.g. `scala.Predef$.classOf`, a type if `is_type`.
    pub fn resolve_full_name(&mut self, full: &str, is_type: bool) -> Option<SymbolId> {
        let (outer, last) = full.rsplit_once('.').unwrap_or(("", full));
        let mut owner = self.u.root();
        for seg in outer.split('.').filter(|s| !s.is_empty()) {
            let decls = self.u.decl(owner, seg).ok()?;
            owner =
                decls.into_iter().find(|&d| matches!(self.u.symbol(d).kind, Kind::Package | Kind::Class))?;
        }
        let decls = match self.u.symbol(owner).kind {
            Kind::Class => self.class_members(owner, last),
            _ => self.u.decl(owner, last).ok()?,
        };
        let mut found = decls.into_iter().filter(|&d| self.u.is_type_symbol(d) == is_type);
        let s = found.next()?;
        found.next().is_none().then_some(s)
    }

    pub fn is_named_package_object(&self, c: SymbolId) -> bool {
        crate::defn::is_named_package_object(self.u, c)
    }

    /// Makes a reference to member `m` from inside its class: an identifier if the class is a
    /// static owner, else a selection from its `this` (dotty's `ref`).
    pub fn member_ref(&mut self, m: SymbolId, span: syntax::ast::Span) -> TNodeId {
        let cls = self.u.symbol(m).owner.expect("a member");
        if self.u.is_static_owner(cls) {
            let prefix = crate::defn::owner_prefix(self.u, cls);
            let tpe = self.u.intern_type(Type::Sym(Some(prefix), m));
            return self.build().ident(m, tpe, span);
        }
        let class_type = crate::defn::type_ref(self.u, cls);
        let this = self.build().this(class_type, span);
        self.build().select(this, m, span)
    }
}
