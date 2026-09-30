//! Classes whose construction runs no code, and their stable constructors (dotty's `NoInits`,
//! `TreeInfo.defKind`, `setStableConstructor`).

use super::Typer;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// Whether statement `s` of a class body initializes nothing (dotty's `defKind` has `NoInits`).
    fn is_no_init_member(&self, s: NodeId) -> bool {
        match self.ast.tree(s) {
            Tree::Empty | Tree::Import { .. } | Tree::TypeDef { .. } | Tree::DefDef { .. } => true,
            Tree::ValDef { rhs, .. } => rhs.is_none(),
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
    /// by its TASTy.
    fn is_no_inits(&self, c: SymbolId) -> bool {
        match self.syms.no_inits.get(&c) {
            Some(&no_inits) => no_inits,
            None => self.u.no_inits(c),
        }
    }

    /// Makes the constructor `constr` of class `cls` stable if `cls` is a trait that initializes
    /// nothing, or a class none of whose base classes does (dotty's `setStableConstructor`).
    pub fn set_stable_constructor(&mut self, cls: SymbolId, constr: NodeId) {
        let stable = if self.u.symbol(cls).flags.has(tasty::tags::TRAIT) {
            self.is_no_inits(cls)
        } else {
            let bases = self.u.linearization(cls);
            bases.iter().all(|&b| self.is_no_inits(b))
        };
        if !stable {
            return;
        }
        let ctor = self.syms.by_tree[&constr];
        let flags = self.syms.flags[&ctor].or(f::STABLE_REALIZABLE);
        self.syms.flags.insert(ctor, flags);
        self.u.set_flags(ctor, crate::symbols::pickled(flags, true));
    }
}
