// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The paths of export clauses: typed ahead in their class, or in an extension an extension
//! method of the same extension (dotty's `processExports`, `exportPathSym`).

use super::exports::Exported;
use crate::ctx::{Ctx, Mode};
use crate::typer::{Pt, Typer};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// Defines the forwarders of the export clauses among `body`, the statements of class `cls`
    /// indexed in `ctx`.
    pub fn process_exports(&mut self, cls: SymbolId, body: &[NodeId], mut ctx: Ctx) {
        let top_level = self.is_named_package_object(cls);
        for &stat in body {
            if let Tree::Export { expr, selectors, ext } = *self.ast.tree(stat) {
                if top_level {
                    self.unsupported(stat, "top-level export");
                    continue;
                }
                let path_method = match ext {
                    None => None,
                    Some(ext) => match self.export_path_method(cls, expr, ext) {
                        Some(m) => Some(m),
                        None => continue,
                    },
                };
                let Some(path) = self.export_path_type(expr, path_method, ctx) else { continue };
                let forwarders = self.export_forwarders(cls, (path, selectors), path_method, ctx);
                self.exports.insert(stat, Exported { path_method, forwarders });
            }
            ctx = self.after_stat(stat, ctx);
        }
    }

    /// The type of export path `expr` typed ahead in `ctx`, a stable path to a value; the result
    /// type of `path_method` in an extension.
    fn export_path_type(&mut self, expr: NodeId, path_method: Option<SymbolId>, ctx: Ctx) -> Option<TypeId> {
        if let Some(m) = path_method {
            self.complete(m);
            let info = self.u.info(m);
            return Some(self.u.result_type(info));
        }
        let path = self.typed(expr, Pt::Any, ctx.with_mode(Mode::QUALIFIER));
        self.typed_ahead(expr, path, ctx);
        let package = self.tpd.sym(path).is_some_and(|s| self.u.symbol(s).kind == Kind::Package);
        if package || !self.is_stable_path(path) {
            self.unsupported(expr, "export prefix that is not a stable path to a value");
            return None;
        }
        Some(self.tpd.tpe(path))
    }

    /// The extension method of class `cls` that path `expr` of an export in extension `ext`
    /// refers to: one of `ext` without clauses of its own.
    fn export_path_method(&mut self, cls: SymbolId, expr: NodeId, ext: NodeId) -> Option<SymbolId> {
        let found = match (self.ast.tree(expr).clone(), self.ast.tree(ext).clone()) {
            (Tree::Ident { name }, Tree::ExtMethods { paramss, methods }) => {
                let name = self.text(name.name).to_owned();
                let n = self.ast.clause_items(paramss).len();
                let own: Vec<u32> = self
                    .ast
                    .items(methods)
                    .iter()
                    .filter(|&&m| {
                        matches!(*self.ast.tree(m), Tree::DefDef { name: d, paramss, .. }
                        if self.text(d.name) == name && self.ast.clause_items(paramss).is_empty())
                    })
                    .map(|&m| self.ast.span(m).point())
                    .collect();
                let decls = self.u.decl(cls, &name).unwrap_or_default();
                let of_ext = |t: &Self, d: &SymbolId| {
                    t.syms.flags.get(d).is_some_and(|fl| fl.is(f::EXTENSION_METHOD))
                        && t.syms.def_trees.get(d).is_some_and(|&tree| {
                            own.contains(&t.ast.span(tree).point()) && t.def_clauses(tree).len() == n
                        })
                };
                decls.into_iter().filter(|d| of_ext(self, d)).collect()
            }
            _ => Vec::new(),
        };
        match found[..] {
            [m] => Some(m),
            _ => {
                self.unsupported(
                    expr,
                    "export qualifier that is not one parameterless method of its extension",
                );
                None
            }
        }
    }
}
