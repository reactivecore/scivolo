// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Classes whose construction runs no code, and their stable constructors (dotty's `NoInits`,
//! `TreeInfo.defKind`, `setStableConstructor`).

use super::Typer;
use crate::symbols::Completion;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// Whether statement `s` of a class body initializes nothing (dotty's `defKind` has `NoInits`):
    /// a synthetic companion object counts as part of its class, an extension method does not;
    /// in a unit behaving as the Scala 2 library, neither do a class and a concrete method.
    fn is_no_init_member(&self, s: NodeId) -> bool {
        match *self.ast.tree(s) {
            Tree::Empty | Tree::Import { .. } => true,
            Tree::TypeDef { rhs, .. } => !self.scala2 || !matches!(self.ast.tree(rhs), Tree::Template { .. }),
            Tree::DefDef { paramss, rhs, .. } if self.scala2 => {
                let mut params = self.ast.clause_items(paramss).iter().flat_map(|&c| self.ast.items(c));
                rhs.is_none()
                    && params.all(|&p| !matches!(self.ast.tree(p), Tree::ValDef { rhs: Some(_), .. }))
            }
            Tree::DefDef { mods, .. } => !self.ast.modifiers(mods).flags.is(f::EXTENSION_METHOD),
            Tree::ValDef { rhs, mods, .. } => {
                let flags = self.ast.modifiers(mods).flags;
                rhs.is_none()
                    || flags.is(syntax::ast::flags::MODULE) && flags.is(syntax::ast::flags::SYNTHETIC)
            }
            _ => false,
        }
    }

    /// Whether a class with parent trees `parents` and statements `body` initializes nothing
    /// itself (dotty's `parentsKind & bodyKind`).
    pub fn has_no_inits(&self, parents: &[NodeId], body: &[NodeId]) -> bool {
        let parent_calls = parents.iter().any(|&p| match self.ast.tree(p) {
            Tree::Apply { args, .. } => !args.is_empty(),
            Tree::Block { .. } => true,
            _ => false,
        });
        !parent_calls && body.iter().all(|&s| self.is_no_init_member(s))
    }

    /// Whether class `c` initializes nothing: a script class by its definition, a library class
    /// by its TASTy, a `java.*` or `javax.*` class other than `Object` or a class of a class file,
    /// not an interface, never.
    pub fn is_no_inits(&self, c: SymbolId) -> bool {
        if let Some(&no_inits) = self.syms.no_inits.get(&c) {
            return no_inits;
        }
        let name = self.u.full_name(c);
        let java = (name.starts_with("java.") || name.starts_with("javax.") || self.u.is_from_classfile(c))
            && name != "java.lang.Object"
            && !self.u.symbol(c).flags.has(tasty::tags::TRAIT);
        !java && self.u.no_inits(c)
    }

    /// Makes the constructor `constr` of class `cls` stable if `cls` is a trait that initializes
    /// nothing, `Some`, a tuple class, or a class none of whose base classes does or is still being
    /// completed (dotty's `setStableConstructor`).
    pub fn set_stable_constructor(&mut self, cls: SymbolId, constr: NodeId) {
        let stable = if self.u.symbol(cls).flags.has(tasty::tags::TRAIT) {
            self.is_no_inits(cls)
        } else if self.u.is_assured_no_inits(cls) {
            true
        } else {
            let bases = self.u.linearization(cls);
            bases.iter().all(|&b| {
                if b != cls {
                    self.complete(b);
                }
                let completing = matches!(self.syms.completions.get(&b), Some(Completion::Completing(_)));
                (b == cls || !completing) && self.is_no_inits(b)
            })
        };
        if !stable {
            return;
        }
        let ctor = self.syms.by_tree[&constr];
        let flags = self.syms.flags[&ctor].or(f::STABLE_REALIZABLE);
        self.syms.set_flags(self.u, ctor, flags);
    }
}
