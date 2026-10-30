// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ImportSuggestions.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Whether dotty may supply a member a qualifier lacks through an extension method or an implicit
//! view, or suggest an import of one (dotty's `tryExtensionOrConversion`, `ImportSuggestions`):
//! one named so, or one whose result may have the member, in the script or a Scala library object;
//! and the objects whose members those searches see.

use super::Typer;
use super::implicits::conversions::Goal;
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
        self.extension_visible(name, ctx)
            || self.finds_completing(name, ctx)
            || self.script_may_supply(Some(name))
            || match self.implicit_scope(site) {
                Ok(scope) => scope.into_iter().any(|c| self.may_view(c.sym, q, Goal::Member(name))),
                Err(_) => true,
            }
            || self.any_root_member(ctx, |t, d| t.may_view(d, q, Goal::Member(name)))
    }

    /// Whether `f` holds for a member of an object in the script, a Scala library object or an
    /// object imported in `ctx`, or those are not all known.
    pub fn any_root_member(&mut self, ctx: Ctx, mut f: impl FnMut(&mut Self, SymbolId) -> bool) -> bool {
        let Some(mut containers) = self.script_classes() else { return true };
        if self.library_objects(&mut containers).is_none()
            || self.imported_objects(ctx, &mut containers, false).is_none()
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
                    if f(self, d) {
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
    /// named `name` if one is given, entered or not.
    pub fn script_may_supply(&self, name: Option<&str>) -> bool {
        let supplies = |ast: &Ast, n: NodeId| {
            match *ast.tree(n) {
            Tree::ExtMethods { methods, .. } => ast.items(methods).iter().any(|&m| {
                matches!(*ast.tree(m), Tree::DefDef { name: TName { name: n, .. }, .. } if Some(self.text(n)) == name)
            }),
            Tree::DefDef { name: n, mods, .. } if ast.modifiers(mods).flags.is(f::EXTENSION_METHOD) => {
                Some(self.text(n.name)) == name
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
    /// packages they import from, to `out`; if `named`, of a package imported from by names only
    /// just its package objects that declare one of them; `None` if one imports from a path other
    /// than a package or an object.
    pub fn imported_objects(&mut self, ctx: Ctx, out: &mut Vec<SymbolId>, named: bool) -> Option<()> {
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l) {
                let info = self.import_info(i)?;
                let (qual, names): (_, Vec<String>) =
                    (info.qual, info.selectors.iter().map(|s| s.name.clone()).collect());
                let site = self.import_site(qual)?;
                let named = named && !names.iter().any(|n| n == "_" || n.is_empty());
                if self.u.symbol(site).kind == Kind::Package {
                    for d in self.u.decls(site).ok()? {
                        let Some(c) = self.object_class(d) else { continue };
                        if !named
                            || self.is_named_package_object(c)
                                && names.iter().any(|n| self.u.decl(c, n).is_ok_and(|ds| !ds.is_empty()))
                        {
                            out.push(c);
                        }
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
