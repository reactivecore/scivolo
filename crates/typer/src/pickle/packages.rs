// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Package clauses and imports (dotty's `PackageDef`, `Import` and `Export` cases of
//! `pickleTree`, `pickleSelectors`).

use super::{Refusal, TreePickler};
use crate::tpd::{List, TNodeId, Tree as T};
use tasty::tags::{BOUNDED, EXPORT, IMPORT, IMPORTED, PACKAGE, RENAMED, TERMREFpkg};
use tasty::write::Name;

impl TreePickler<'_> {
    /// The package clause of `pid` with `stats`, the root package's as the empty package's
    /// unless written `_root_`.
    pub fn package(&mut self, pid: TNodeId, stats: List) -> Result<(), Refusal> {
        let tpe = self.tpd.get(pid).tpe.ok_or("a package clause without a type")?;
        let stats = self.tpd.items(stats).to_vec();
        let root = matches!(*self.tpd.tree(pid), T::Ident { name } if self.names.text(name.name) == names::nme::ROOT_PKG);
        let empty = self.tpd.sym(pid) == Some(self.u.root()) && !root;
        self.with_length(PACKAGE, &mut |p| {
            match empty {
                true => {
                    p.byte(TERMREFpkg);
                    p.package_name(p.u.root());
                }
                false => p.ty(tpe, false)?,
            }
            p.pre_register(&stats);
            stats.iter().try_for_each(|&s| p.tree(s))
        })
    }

    /// Import or export `expr` with `selectors`.
    pub fn import(&mut self, expr: TNodeId, selectors: List, export: bool) -> Result<(), Refusal> {
        let selectors = self.tpd.items(selectors).to_vec();
        self.with_length(if export { EXPORT } else { IMPORT }, &mut |p| {
            p.tree(expr)?;
            for &sel in &selectors {
                let T::ImportSelector { imported, renamed, bound } = *p.tpd.tree(sel) else {
                    return Err("an import selector of another kind");
                };
                p.selector(IMPORTED, imported)?;
                if let Some(r) = renamed.filter(|&r| matches!(p.tpd.tree(r), T::Ident { .. })) {
                    p.selector(RENAMED, r)?;
                }
                if let Some(b) = bound {
                    p.byte(BOUNDED);
                    p.tree(b)?;
                }
            }
            Ok(())
        })
    }

    fn selector(&mut self, tag: u8, id: TNodeId) -> Result<(), Refusal> {
        let T::Ident { name } = *self.tpd.tree(id) else {
            return Err("an import selector that is not a name");
        };
        self.buf.register_tree(id.index() as u32);
        self.byte(tag);
        self.name(&Name::utf8(self.names.text(name.name)));
        Ok(())
    }
}
