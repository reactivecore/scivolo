//! The implicits eligible for a search (dotty's `ContextualImplicits` and `implicitScope`): those
//! visible in a context, innermost first, and those of the companions of a type's parts.

use super::super::Typer;
use crate::ctx::{Ctx, Level};
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
    fn is_implicit(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        matches!(sym.kind, Kind::Val | Kind::Def) && (sym.flags.has(IMPLICIT) || sym.flags.has(GIVEN))
    }

    /// The implicit members of class `c` and its base classes, the private ones of `c` if
    /// `own_private`.
    fn implicit_members(&mut self, c: SymbolId, own_private: bool) -> Vec<SymbolId> {
        self.complete(c);
        let mut out = Vec::new();
        for base in self.u.linearization(c) {
            self.complete(base);
            for d in self.u.decls(base).unwrap_or_default() {
                if self.is_implicit(d)
                    && (own_private && base == c || !self.u.symbol(d).flags.has(PRIVATE))
                    && !out.contains(&d)
                {
                    out.push(d);
                }
            }
        }
        out
    }

    /// The implicits visible in `ctx`, by nesting level, innermost first, an import at the level
    /// of the scope it is in; `Err` if seeing them needs a symbol whose completion is in progress
    /// (dotty's cyclic reference).
    pub fn contextual_implicits(&mut self, ctx: Ctx) -> Result<Vec<Vec<Candidate>>, &'static str> {
        let mut levels: Vec<(Option<crate::ctx::LevelId>, Vec<Candidate>)> = Vec::new();
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            let (scope, owner, import) = match self.levels.get(l) {
                Level::Scope(entries) => (entries.iter().map(|&(_, s)| s).collect(), None, None),
                Level::Members(owner) => (Vec::new(), Some(*owner), None),
                Level::Import(i) => (Vec::new(), None, Some(*i)),
            };
            let mut found: Vec<Candidate> = scope
                .into_iter()
                .filter(|&s| self.is_implicit(s))
                .map(|sym| Candidate { sym, prefix: None })
                .collect();
            if let Some(owner) = owner.filter(|&o| self.u.symbol(o).kind == Kind::Class) {
                let this = crate::defn::type_ref(self.u, owner);
                let prefix = Some(self.u.intern_type(Type::This(this)));
                found.extend(
                    self.implicit_members(owner, true).into_iter().map(|sym| Candidate { sym, prefix }),
                );
            }
            if let Some(i) = import {
                found.extend(self.imported_implicits(i)?);
            }
            let key = self.levels.def_scope(l);
            match levels.last_mut() {
                Some((k, group)) if *k == key => group.extend(found),
                _ => levels.push((key, found)),
            }
            level = self.levels.outer(l);
        }
        Ok(levels.into_iter().map(|(_, g)| g).filter(|g| !g.is_empty()).collect())
    }

    /// The implicits import `i` makes visible: implicit members by a wildcard, givens by `given`,
    /// either by name.
    fn imported_implicits(&mut self, i: usize) -> Result<Vec<Candidate>, &'static str> {
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
            let named = names.iter().any(|(n, r)| *n == name && r != "_");
            if !excluded && (named || if given { givens } else { wildcard }) {
                out.push(Candidate { sym, prefix: Some(qual) });
            }
        }
        Ok(out)
    }

    /// The implicits of the companions of the classes `t` refers to and of their base classes.
    pub fn implicit_scope(&mut self, t: TypeId) -> Vec<Candidate> {
        let mut classes = Vec::new();
        let t = self.zonk(t);
        self.u.map_type(t, &mut |u, part| {
            if matches!(u.types().get(part), Type::Sym(..) | Type::TypeRef(..))
                && let Some(s) = u.class_symbol(part)
                && !classes.contains(&s)
            {
                classes.push(s);
            }
            None
        });
        let mut out = Vec::new();
        for c in classes {
            for base in self.u.linearization(c) {
                let Some(module) = self.companion_module(base) else { continue };
                let Some(module_class) = self.u.module_class(module) else { continue };
                let prefix = Some(crate::defn::type_ref(self.u, module));
                for sym in self.implicit_members(module_class, false) {
                    let candidate = Candidate { sym, prefix };
                    if !out.contains(&candidate) {
                        out.push(candidate);
                    }
                }
            }
        }
        out
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
        let through_this = c.prefix.is_some_and(|p| matches!(self.u.types().get(p), Type::This(_)));
        if !through_this && !self.term_prefix_is_elidable(c.prefix, c.sym, ctx) {
            return Err("implicit through a path");
        }
        Ok(self.ref_(c.prefix, c.sym, tpe, span, ctx))
    }

    /// The companion object of class `c`, if `c` is not nested in a class other than an object's.
    fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(c).owner?;
        if self.u.is_class(owner) && !self.u.is_module_class(owner) {
            return None;
        }
        self.u.companion_module(c)
    }
}
