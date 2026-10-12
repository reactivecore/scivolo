//! Identifiers (dotty's `typedIdent`) and the references they make.

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
    /// Whether `c` is a package, or an object whose owners are all packages or objects.
    pub fn is_static_owner(&self, c: SymbolId) -> bool {
        let s = self.u.symbol(c);
        match s.kind {
            Kind::Package => true,
            Kind::Class if s.flags.has(tasty::tags::OBJECT) => {
                s.owner.is_none_or(|o| self.is_static_owner(o))
            }
            _ => false,
        }
    }

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
        if text == "_root_" && !name.is_type {
            let root = self.u.root();
            let tpe = self.u.intern_type(Type::Package(root));
            let span = self.ast.span(id);
            return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(root));
        }
        let found = match self.find_ref(&text, name.is_type, ctx) {
            Ok(Some(found)) => found,
            Ok(None) if matches!(pt, Pt::Fun(_)) && !name.is_type => {
                return self.proxy_ident(id, name, pt, ctx);
            }
            Ok(None) => {
                return match self.not_found(id, name, ctx) {
                    Some(error) => error,
                    None => self.unsupported(id, &format!("unresolved name {text}")),
                };
            }
            Err("constructor proxy") if matches!(pt, Pt::Fun(_)) => {
                return self.proxy_ident(id, name, pt, ctx);
            }
            Err(what) => return self.unsupported(id, what),
        };
        let sym = match self.choose(&found.syms, found.prefix, pt) {
            Ok(sym) => sym,
            Err(what) => return self.unchosen(id, what, pt),
        };
        if let Err(what) = self.check_reference(sym, found.prefix, ctx) {
            return self.unsupported(id, what);
        }
        if let Err(what) = self.check_value(sym, name, matches!(pt, Pt::Fun(_)), ctx) {
            return self.unsupported(id, what);
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
        if !self.prefix_is_elidable(tpe, ctx) {
            if name.is_type {
                return self.type_tree(tpe, span, false);
            }
            if let Some(this) = self.module_this(sym, span, ctx) {
                return this;
            }
            return self.this_select(span, name, tpe, sym);
        }
        self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym))
    }

    /// A reference desugaring made to the library symbol of full name `sym`: a type tree for a
    /// type (dotty's `TypeTree(sym.typeRef)`), else an identifier of its simple name (`ref(sym)`).
    pub fn typed_full_ref(&mut self, id: NodeId, sym: TName) -> TNodeId {
        let full = self.text(sym.name).to_owned();
        let Some(s) = self.resolve_full_name(&full, sym.is_type) else {
            return self.unsupported(id, &format!("reference to {full}"));
        };
        let span = self.ast.span(id);
        if sym.is_type {
            let tpe = crate::defn::type_ref(self.u, s);
            return self.type_tree(tpe, span, false);
        }
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

    pub fn ref_type(&mut self, prefix: Option<TypeId>, sym: SymbolId) -> TypeId {
        crate::defn::ref_type(self.u, prefix, sym)
    }

    pub fn is_named_package_object(&self, c: SymbolId) -> bool {
        crate::defn::is_named_package_object(self.u, c)
    }
}
