// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Anonymous mirrors (dotty's `anonymousMirror`): `new Object with Serializable { type
//! MirroredMonoType = M }`, marked for the lowering to add the mirror's parent and methods.

use super::super::{Pt, Typer};
use super::mirror_types::{MISSING, MirrorSynthesis};
use crate::tpd::{AnonMirror, TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, TName, Tree};
use tasty::TypeId;

impl MirrorSynthesis {
    /// Types the anonymous sum (`sum`) or product mirror of `mono`.
    pub fn anonymous(self, t: &mut Typer<'_>, mono: TypeId, sum: bool) -> Result<TNodeId, &'static str> {
        let object = crate::defn::type_ref(t.u, t.u.builtins().object);
        let serializable = t.resolve_full_name("java.io.Serializable", true).ok_or(MISSING)?;
        let serializable = crate::defn::type_ref(t.u, serializable);
        let parents = [self.spliced_type(t, object), self.spliced_type(t, serializable)];
        let parents = t.ast.list(&parents);
        let rhs = self.spliced_type(t, mono);
        let name = TName { name: t.name("MirroredMonoType"), is_type: true };
        let mods = t.ast.mods(Modifiers::with_flags(f::EMPTY));
        let mono_def = self.node(t, Tree::TypeDef { name, rhs, mods });
        let body = t.ast.list(&[mono_def]);
        let init = TName { name: t.name("<init>"), is_type: false };
        let clause = t.ast.list(&[]);
        let paramss = t.ast.clauses(&[clause]);
        let tpt = self.node(t, Tree::TypeTree);
        let mods = t.ast.mods(Modifiers::with_flags(f::EMPTY));
        let constr = self.node(t, Tree::DefDef { name: init, paramss, tpt, rhs: None, mods });
        let template =
            Tree::Template { constr, parents_or_derived: parents, self_: None, body, derived: None };
        let template = self.node(t, template);
        let new = self.node(t, Tree::New { tpt: template });
        let typed = t.typed(new, Pt::Any, self.ctx);
        let T::Block { stats, .. } = *t.tpd.tree(typed) else {
            return Err("anonymous Mirror outside the subset");
        };
        let class = t.tpd.items(stats).first().and_then(|&d| t.tpd.sym(d));
        let class = class.ok_or("anonymous Mirror outside the subset")?;
        let kind = if sum { AnonMirror::Sum } else { AnonMirror::Product };
        t.tpd.mark_anon_mirror(class, kind);
        Ok(typed)
    }

    /// Adds an untyped node standing for a type tree of `tpe`.
    fn spliced_type(self, t: &mut Typer<'_>, tpe: TypeId) -> NodeId {
        let tree = t.type_tree(tpe, self.at, false);
        t.spliced(tree)
    }

    /// Adds untyped `tree` to the typer's syntax tree.
    fn node(self, t: &mut Typer<'_>, tree: Tree) -> NodeId {
        let node = t.ast.node(tree);
        t.ast.with_span(node, self.at);
        node
    }
}
