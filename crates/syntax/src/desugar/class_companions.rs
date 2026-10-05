//! What a class definition adds next to the class: the synthetic companion object and the
//! conversion method of an implicit class (from dotty's `classDef`).

use super::class_parts::ClassParts;
use super::{Desugar, Owner};
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The access and `final` of the class, with no written modifiers and annotations.
    fn companion_mods(&self, c: &ClassParts) -> Modifiers {
        let flags = c.mods.flags.and(flags::ACCESS_FLAGS.or(flags::FINAL)).to_common_flags();
        Modifiers { flags, private_within: c.mods.private_within, ..Modifiers::default() }
    }

    /// The expansion of the synthetic companion `object C extends AnyRef derives ... { defs }`.
    pub fn companion_defs(&mut self, c: &ClassParts, defs: &[NodeId], derived: &[NodeId]) -> Vec<NodeId> {
        let any_ref = self.sym_ref("scala.AnyRef", true);
        let constr = self.empty_constructor();
        let n = derived.len() as u32;
        let parents_or_derived = self.list(&[&[any_ref][..], derived].concat());
        let body = self.list(defs);
        let imp = self.ast.node(Tree::Template {
            constr,
            parents_or_derived,
            self_: None,
            body,
            derived: (n > 0).then_some(n),
        });
        let m = self.companion_mods(c).or(flags::SYNTHETIC);
        let mods = self.ast.mods(m);
        let name = TName { is_type: false, ..c.name };
        let mdef = self.ast.node(Tree::ModuleDef { name, imp, mods });
        let expanded = self.module_def(mdef, c.is(flags::MODULE));
        let expanded = self.ast.respan(expanded, self.ast.span(c.cdef));
        let mdefs = self.flatten(&[expanded]);
        if n > 0 {
            for &t in &mdefs {
                if matches!(self.ast.tree(t), Tree::TypeDef { .. }) {
                    self.ast.add_attrs(t, Attrs::DERIVING_COMPANION);
                }
            }
        }
        mdefs
    }

    /// The conversion method of an implicit class.
    pub fn implicit_wrappers(&mut self, c: &mut ClassParts) -> Vec<NodeId> {
        if !c.mods.is_one_of(flags::GIVEN_OR_IMPLICIT) {
            return Vec::new();
        }
        let problem = if self.owner == Owner::Package {
            Some("An implicit class may not be top-level")
        } else if c.is(flags::TRAIT) {
            Some("implicit modifier cannot be used for types or traits")
        } else if c.is_case_class() {
            Some("A case class may not be defined as implicit")
        } else if c.arity() != 1 && !c.is(flags::GIVEN) {
            Some("Implicit classes must accept exactly one primary constructor parameter")
        } else {
            None
        };
        if let Some(msg) = problem {
            self.error(c.cdef, msg);
            return Vec::new();
        }
        let mut paramss: Vec<Vec<NodeId>> =
            c.constr_vparamss.iter().map(|ps| ps.iter().map(|&p| self.ast.copy_node(p)).collect()).collect();
        if paramss.first().is_some_and(Vec::is_empty) {
            paramss.remove(0);
        }
        let tparams = self.fresh_type_params(&c.constr_tparams);
        let clauses = self.join_params(&tparams, &paramss);
        let creator = self.creator_expr(c);
        let kept = c.mods.flags.to_term_flags().and(flags::GIVEN_OR_IMPLICIT.or(flags::INLINE));
        let m = self.companion_mods(c).or(kept.or(flags::FINAL));
        let mods = self.ast.mods(m);
        let name = TName { is_type: false, ..c.name };
        let wrapper = self.method(name, &clauses, Some(c.class_type_ref), creator, mods);
        vec![self.ast.respan(wrapper, self.ast.span(c.cdef))]
    }
}
