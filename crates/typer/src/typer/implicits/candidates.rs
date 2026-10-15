//! Implicit members of classes and imports, and references to them.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::tags::{GIVEN, IMPLICIT, PRIVATE};
use tasty::{Kind, SymbolId, Type, TypeId};

/// An implicit value or method and the prefix it is referenced through.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Candidate {
    pub sym: SymbolId,
    pub prefix: Option<TypeId>,
}

impl Typer<'_> {
    pub fn is_implicit(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        sym.kind.is_term() && (sym.flags.has(IMPLICIT) || sym.flags.has(GIVEN))
    }

    /// The implicit members of class `c` and its base classes, the private ones of `c` if
    /// `own_private`, those overridden in a class before them in the linearization left out.
    pub fn implicit_members(&mut self, c: SymbolId, own_private: bool) -> Vec<SymbolId> {
        self.complete(c);
        let site = crate::defn::type_ref(self.u, c);
        let lin = self.u.linearization(c);
        let mut out = Vec::new();
        for (i, &base) in lin.iter().enumerate() {
            self.complete(base);
            for d in self.u.decls(base).unwrap_or_default() {
                if self.is_implicit(d)
                    && (own_private && base == c || !self.u.symbol(d).flags.has(PRIVATE))
                    && !out.contains(&d)
                    && !self.overridden_in(site, &lin[..i], d)
                {
                    out.push(d);
                }
            }
        }
        out
    }

    /// Whether a class of `bases` declares a member that overrides `d` as seen from `site`.
    fn overridden_in(&mut self, site: TypeId, bases: &[SymbolId], d: SymbolId) -> bool {
        let name = self.u.symbol(d).name.clone();
        for &b in bases {
            for m in self.u.decl(b, &name).unwrap_or_default() {
                let s = self.u.symbol(m);
                let term = s.kind.is_term() && s.kind != Kind::Param;
                if m != d && term && !s.flags.has(PRIVATE) && self.same_member(site, m, d) {
                    return true;
                }
            }
        }
        false
    }

    /// The implicits import `i` makes visible, with the names they are visible under: implicit
    /// members by a wildcard, both by `given`, either by name.
    pub fn imported_implicits(&mut self, i: usize) -> Result<Vec<(Candidate, String)>, &'static str> {
        let Some(info) = self.import_info(i) else { return Ok(Vec::new()) };
        let qual = info.qual;
        let names: Vec<(String, String)> =
            info.selectors.iter().map(|s| (s.name.clone(), s.rename.clone())).collect();
        let Some(site) = self.import_site(qual) else { return Ok(Vec::new()) };
        let aliased_object = |t: &mut Self| {
            let widened = t.u.widen(qual);
            t.u.class_symbol(widened).filter(|&c| t.u.symbol(c).flags.has(tasty::tags::OBJECT))
        };
        let Some(class) = self
            .u
            .module_class(site)
            .or_else(|| (self.u.symbol(site).kind == Kind::Class).then_some(site))
            .or_else(|| aliased_object(self))
        else {
            if self.u.symbol(site).kind.is_term() {
                return Err("implicit import from a value other than an object");
            }
            return Ok(Vec::new());
        };
        for (name, _) in names.iter().filter(|(n, _)| !n.is_empty() && n != "_") {
            for m in self.class_members(class, name) {
                if matches!(self.syms.completions.get(&m), Some(crate::symbols::Completion::Completing(_))) {
                    return Err("implicit search through a definition being completed");
                }
            }
        }
        let wildcard = names.iter().any(|(n, _)| n == "_");
        let givens = names.iter().any(|(n, _)| n.is_empty());
        let mut out = Vec::new();
        for sym in self.implicit_members(class, false) {
            let name = self.u.symbol(sym).name.clone();
            let given = self.u.symbol(sym).flags.has(GIVEN);
            let excluded = names.iter().any(|(n, r)| *n == name && r == "_");
            let named = names.iter().find(|(n, r)| *n == name && r != "_").map(|(_, r)| r.clone());
            if !excluded && (named.is_some() || givens || (!given && wildcard)) {
                out.push((Candidate { sym, prefix: Some(qual) }, named.unwrap_or(name)));
            }
        }
        Ok(out)
    }

    /// A reference to candidate `c` at `span` from `ctx` (dotty's `tpd.ref`): an identifier, or a
    /// selection from `this`.
    pub fn candidate_ref(&mut self, c: Candidate, span: Span, ctx: Ctx) -> Result<TNodeId, &'static str> {
        if self.needs_inline_accessor(c.sym, ctx) {
            return Err("non-public implicit in an inline method");
        }
        let package_object = self.u.symbol(c.sym).owner.filter(|&o| self.is_named_package_object(o));
        let through_object = |t: &mut Self, p: Option<Type>, o: SymbolId| match p {
            Some(Type::Package(_)) => true,
            Some(Type::Sym(_, m)) => t.u.module_class(m) == Some(o),
            Some(Type::This(c)) => match *t.u.types().get(c) {
                Type::Package(_) => true,
                Type::Sym(_, s) => s == o || t.u.symbol(s).kind == tasty::Kind::Package,
                _ => false,
            },
            _ => false,
        };
        let prefix = c.prefix.map(|p| self.u.types().get(p).clone());
        let tpe = match (prefix.clone(), package_object) {
            (_, Some(o)) if through_object(self, prefix, o) => {
                let class = crate::defn::type_ref(self.u, o);
                let this = self.u.intern_type(Type::This(class));
                self.u.intern_type(Type::Sym(Some(this), c.sym))
            }
            _ => self.ref_type(c.prefix, c.sym),
        };
        let through_path = !self.term_prefix_is_elidable(c.prefix, c.sym, ctx);
        if through_path && c.prefix.and_then(|p| self.path_tree(p, span, 8)).is_none() {
            return Err("implicit through a path");
        }
        Ok(self.ref_(c.prefix, c.sym, tpe, span, ctx))
    }
}
