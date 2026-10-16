//! The children of a sealed class (dotty's `children`, `isGenericSum`), and their types as the
//! elements of its sum mirror.

use super::super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{ABSTRACT, CASE, CONTRAVARIANT, ENUM, OBJECT, SEALED};
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The children of sealed class `s` in the order of their definitions: classes, and objects
    /// by their values; of an enum, its cases. Refused for a sealed class of the library.
    pub fn sealed_children(&mut self, s: SymbolId) -> Result<Vec<SymbolId>, &'static str> {
        let Some(&def) = self.syms.def_trees.get(&s) else {
            return Err("sum Mirror of a library's sealed class");
        };
        if self.u.symbol(s).flags.has(ENUM) {
            let companion = self.u.companion_module(s).and_then(|m| self.u.module_class(m));
            let decls = companion.map(|c| self.u.symbol(c).decls.clone()).unwrap_or_default();
            return Ok(decls
                .into_iter()
                .filter(|&d| {
                    let (kind, flags) = (self.u.symbol(d).kind, self.u.symbol(d).flags);
                    let case =
                        kind == Kind::Class && !flags.has(OBJECT) || kind == Kind::Val && !flags.has(OBJECT);
                    case && flags.has(CASE) && flags.has(ENUM)
                })
                .collect());
        }
        let file = self.file_of(self.ast.span(def));
        let mut found: Vec<(u32, SymbolId)> = Vec::new();
        let defs: Vec<(SymbolId, syntax::ast::NodeId)> =
            self.syms.def_trees.iter().map(|(&k, &v)| (k, v)).collect();
        for (c, node) in defs {
            let span = self.ast.span(node);
            if self.u.symbol(c).kind != Kind::Class || c == s || self.file_of(span) != file {
                continue;
            }
            self.complete(c);
            if !self.u.parents(c).into_iter().any(|p| self.u.class_symbol(p) == Some(s)) {
                continue;
            }
            let child = match self.u.is_module_class(c) {
                true => self.u.source_module(c).ok_or("sealed child object without its value")?,
                false => c,
            };
            found.push((span.start(), child));
        }
        found.sort_by_key(|&(start, _)| start);
        Ok(found.into_iter().map(|(_, c)| c).collect())
    }

    /// Whether sealed class `s` has a sum mirror (dotty's `isGenericSum`), its children visible
    /// from `ctx` unless its companion is the mirror.
    pub fn is_generic_sum(&mut self, s: SymbolId, companion: bool, ctx: Ctx) -> Result<bool, &'static str> {
        let flags = self.u.symbol(s).flags;
        if !flags.has(SEALED) || !(flags.has(ABSTRACT) || self.u.is_trait(s)) {
            return Ok(false);
        }
        let children = self.sealed_children(s)?;
        if children.is_empty() {
            return Ok(false);
        }
        for c in children {
            if self.u.symbol(c).name.starts_with("$anon") {
                return Ok(false);
            }
            let owner = self.u.symbol(c).owner.ok_or("sealed child without owner")?;
            if !self.is_visible_to_parent(s, owner) {
                return Ok(false);
            }
            if !companion && !self.is_visible_to_scope(owner, ctx.owner) {
                return Err("sum Mirror of a child visible through a prefix only");
            }
            if !self.u.is_class(c) {
                continue;
            }
            let fits = self.is_generic_product(c, ctx)
                || self.u.symbol(c).flags.has(SEALED) && self.is_generic_sum(c, companion, ctx)?;
            if !fits {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The type of child `c` of the sum mirrored as `target` (dotty's `solve`): an object's or
    /// value's singleton type, a class with its type parameters solved from `C[Xs] <: target`,
    /// contravariant ones minimized, the others maximized.
    pub fn child_type(&mut self, c: SymbolId, target: TypeId) -> Result<TypeId, &'static str> {
        let raw = crate::defn::type_ref(self.u, c);
        let params = match self.u.is_class(c) {
            true => self.u.type_params(c),
            false => Vec::new(),
        };
        if params.is_empty() {
            return Ok(raw);
        }
        let mark = self.constraint.mark();
        let vars: Vec<TypeId> = params.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let applied = self.u.intern_type(Type::Applied(raw, vars.clone()));
        let solved = self.conforms(applied, target) && self.instantiate_child_vars(&params, &vars).is_ok();
        let t = self.zonk(applied);
        self.constraint.rollback(mark);
        match solved && self.is_fully_defined(t) {
            true => Ok(t),
            false => Err("sum Mirror of a child whose type parameters are not solved"),
        }
    }

    fn instantiate_child_vars(&mut self, params: &[SymbolId], vars: &[TypeId]) -> Result<(), &'static str> {
        for (&p, &v) in params.iter().zip(vars) {
            let Type::Var(v) = *self.u.types().get(v) else { continue };
            if self.constraint.instance(v).is_none() {
                let below = self.u.symbol(p).flags.has(CONTRAVARIANT);
                self.instantiate_var(v, below)?;
            }
        }
        Ok(())
    }

    /// Whether `owner`, where a child of `s` is defined, encloses `s` or is an object visible so.
    fn is_visible_to_parent(&mut self, s: SymbolId, owner: SymbolId) -> bool {
        self.is_contained_in(s, owner)
            || self.u.is_module_class(owner)
                && self.u.symbol(owner).owner.is_some_and(|o| self.is_visible_to_parent(s, o))
    }

    /// Whether `owner` encloses `site` or is an object visible so.
    fn is_visible_to_scope(&mut self, owner: SymbolId, site: SymbolId) -> bool {
        self.is_contained_in(site, owner)
            || self.u.is_module_class(owner)
                && self.u.symbol(owner).owner.is_some_and(|o| self.is_visible_to_scope(o, site))
    }

    /// Whether `s` is `owner` or owned by it, transitively.
    fn is_contained_in(&self, s: SymbolId, owner: SymbolId) -> bool {
        let mut cur = Some(s);
        while let Some(c) = cur {
            if c == owner {
                return true;
            }
            cur = self.u.symbol(c).owner;
        }
        false
    }
}
