//! Whether dotty may supply a member a qualifier lacks through an extension method or an implicit
//! view, or suggest an import of one (dotty's `tryExtensionOrConversion`, `ImportSuggestions`):
//! one named so, or one whose result may have the member, in the script or a Scala library object.

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::symbols::Completion;
use rustc_hash::FxHashSet;
use syntax::ast::flags as f;
use syntax::ast::{Ast, NodeId, TName, Tree};
use tasty::tags::OBJECT;
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// Whether an extension method or implicit view may give class `q` of type `site` member
    /// `name` in `ctx`, or dotty's import suggestions may name one.
    pub fn may_supply(&mut self, (q, site): (SymbolId, TypeId), name: &str, ctx: Ctx) -> bool {
        if self.extension_visible(name, ctx)
            || self.finds_completing(name, ctx)
            || self.script_may_supply(name)
            || self.implicit_scope(site).into_iter().any(|c| self.may_view(c.sym, q, name))
        {
            return true;
        }
        let Some(mut containers) = self.script_classes() else { return true };
        if self.library_objects(&mut containers).is_none()
            || self.imported_objects(ctx, &mut containers).is_none()
        {
            return true;
        }
        let mut seen = FxHashSet::default();
        while let Some(c) = containers.pop() {
            if !seen.insert(c) {
                continue;
            }
            for base in self.u.linearization(c) {
                let Ok(decls) = self.u.decls(base) else { return true };
                for d in decls {
                    containers.extend(self.object_class(d));
                    if self.may_view(d, q, name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The script classes and the objects their base classes declare; `None` if the parents of
    /// one are not known yet.
    fn script_classes(&mut self) -> Option<Vec<SymbolId>> {
        let mut script: Vec<SymbolId> = self.syms.flags.keys().copied().collect();
        script.extend(self.syms.by_tree.values().copied());
        script.retain(|&s| self.u.symbol(s).kind == Kind::Class);
        let mut out = Vec::new();
        for s in script {
            if self.u.parents(s).is_empty() && self.has_parent_trees(s) {
                return None;
            }
            out.push(s);
            for base in self.u.linearization(s) {
                let decls = self.u.decls(base).ok()?;
                out.extend(decls.into_iter().filter_map(|d| self.object_class(d)));
            }
        }
        Some(out)
    }

    /// Adds the classes of the top-level objects of the `scala` packages but Scala Native's to
    /// `out`.
    fn library_objects(&mut self, out: &mut Vec<SymbolId>) -> Option<()> {
        let mut packages = vec![self.u.package("scala")?];
        while let Some(p) = packages.pop() {
            for d in self.u.decls(p).ok()? {
                match self.u.symbol(d).kind {
                    Kind::Package if self.u.symbol(d).name != "scalanative" => packages.push(d),
                    _ => out.extend(self.object_class(d)),
                }
            }
        }
        Some(())
    }

    /// Whether the unit has a given or implicit definition or parameter, or an extension method
    /// named `name`, entered or not.
    fn script_may_supply(&self, name: &str) -> bool {
        let supplies = |ast: &Ast, n: NodeId| {
            match *ast.tree(n) {
            Tree::ExtMethods { methods, .. } => ast.items(methods).iter().any(|&m| {
                matches!(*ast.tree(m), Tree::DefDef { name: TName { name: n, .. }, .. } if self.text(n) == name)
            }),
            Tree::DefDef { name: n, mods, .. } if ast.modifiers(mods).flags.is(f::EXTENSION_METHOD) => {
                self.text(n.name) == name
            }
            _ => ast.mods_of(n).is_some_and(|m| ast.modifiers(m).flags.is(f::GIVEN_OR_IMPLICIT)),
        }
        };
        self.ast.exists_subtree(self.unit, &supplies)
    }

    /// Whether script class `c`, its parents not entered yet, has parents written or may have.
    fn has_parent_trees(&self, c: SymbolId) -> bool {
        let tree = match self.syms.completions.get(&c) {
            Some(&Completion::Pending { tree, .. } | &Completion::Completing(tree)) => tree,
            _ => return false,
        };
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return true };
        !matches!(*self.ast.tree(rhs), Tree::Template { parents_or_derived, derived: None, .. }
            if self.ast.items(parents_or_derived).is_empty())
    }

    /// The class of `d` if it is an object.
    fn object_class(&mut self, d: SymbolId) -> Option<SymbolId> {
        let sym = self.u.symbol(d);
        if sym.kind == Kind::Val && sym.flags.has(OBJECT) { self.u.module_class(d) } else { None }
    }

    /// Whether `name` in `ctx` refers to a definition whose completion is in progress, which dotty's
    /// search for an extension method may find cyclic.
    fn finds_completing(&mut self, name: &str, ctx: Ctx) -> bool {
        match self.find_ref(name, false, ctx) {
            Err(_) => true,
            Ok(None) => false,
            Ok(Some(found)) => found
                .syms
                .iter()
                .any(|s| matches!(self.syms.completions.get(s), Some(Completion::Completing(_)))),
        }
    }

    /// Adds the classes of the objects the imports in `ctx` import from, or of the objects of the
    /// packages they import from, to `out`; `None` if one imports from a path other than a package
    /// or an object.
    fn imported_objects(&mut self, ctx: Ctx, out: &mut Vec<SymbolId>) -> Option<()> {
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l) {
                let qual = self.import_info(i)?.qual;
                let site = self.import_site(qual)?;
                if self.u.symbol(site).kind == Kind::Package {
                    for d in self.u.decls(site).ok()? {
                        out.extend(self.object_class(d));
                    }
                } else {
                    out.push(self.object_class(site)?);
                }
            }
            level = self.levels.outer(l);
        }
        Some(())
    }
}
