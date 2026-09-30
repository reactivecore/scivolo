//! A class definition as the class with accessors and synthetic members, its companion
//! object and its implicit wrapper (the second half of dotty's `classDef`).

use super::class_parts::ClassParts;
use super::{Desugar, Owner};
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The expansion of class definition `cdef`.
    pub fn class_def(&mut self, cdef: NodeId) -> NodeId {
        let mut c = self.class_parts(cdef);
        let is_object = c.is(flags::MODULE);
        let is_case_class = c.is_case_class();
        let is_value_class = c.parents.first().is_some_and(|&p| self.is_any_val(p));
        let case_meths = if is_case_class { self.case_class_meths(&mut c) } else { Vec::new() };
        let mut parents = c.parents.clone();
        if c.is(flags::CASE) && !c.is(flags::ENUM) {
            for name in ["Product", "Serializable"] {
                let name = self.type_name(name);
                parents.push(self.scala_dot(name));
            }
        }
        let (cls_derived, companion_derived) =
            if is_object { (c.derived.clone(), Vec::new()) } else { (Vec::new(), c.derived.clone()) };
        let members = c.default_getters.clone();
        let companions = if is_case_class {
            let meths = self.case_companion_meths(&mut c);
            self.companion_defs(&c, &[meths, members].concat(), &companion_derived)
        } else if !members.is_empty() || !companion_derived.is_empty() || is_value_class && !is_object {
            self.companion_defs(&c, &members, &companion_derived)
        } else {
            Vec::new()
        };
        let wrappers = self.implicit_wrappers(&mut c);
        let cdef1 = self.class_with_members(&c, parents, cls_derived, case_meths);
        self.flat_tree(&[&[cdef1][..], &companions, &wrappers].concat())
    }

    /// The class of `c` with its parameter accessors, body and synthetic members.
    fn class_with_members(
        &mut self,
        c: &ClassParts,
        parents: Vec<NodeId>,
        derived: Vec<NodeId>,
        case_meths: Vec<NodeId>,
    ) -> NodeId {
        let Tree::Template { self_, .. } = *self.ast.tree(c.template) else { unreachable!("a template") };
        let self_ = self_.map(|s| self.class_self(c, s));
        let mut body = Vec::new();
        for (&d, &orig) in c.derived_tparams.iter().zip(&c.original_tparams) {
            let m = self.ast.modifiers(self.ast.mods_of(orig).expect("modifiers")).clone();
            body.push(self.ast.with_mods(d, m));
        }
        let originals: Vec<NodeId> = c.original_vparamss.iter().flatten().copied().collect();
        let deriveds: Vec<(usize, NodeId)> = c
            .derived_vparamss
            .iter()
            .enumerate()
            .flat_map(|(i, ps)| ps.iter().map(move |&p| (i, p)))
            .collect();
        for (&(clause, d), &orig) in deriveds.iter().zip(&originals) {
            let mut m = self.ast.modifiers(self.ast.mods_of(orig).expect("modifiers")).clone();
            if clause == 0 && c.is_case_class() {
                m = m.or(flags::CASE_ACCESSOR);
            }
            body.push(self.ast.with_mods(d, m));
        }
        body.extend_from_slice(&c.body);
        body.extend(case_meths);
        if c.mods.flags.is_all_of(flags::GIVEN.or(flags::INLINE).or(flags::TRANSPARENT)) {
            self.error(c.cdef, "inline given instances cannot be transparent");
        }
        let mut mods = c.mods.clone();
        if mods.is(flags::GIVEN) {
            mods = mods.and_not(flags::INLINE.or(flags::TRANSPARENT)).or(flags::SYNTHETIC);
        }
        let tracked = body.iter().any(|&s| {
            matches!(self.ast.tree(s), Tree::ValDef { .. } | Tree::DefDef { .. })
                && self.ast.mods_of(s).is_some_and(|m| self.ast.modifiers(m).is(flags::TRACKED))
        });
        if tracked {
            mods = mods.or(flags::DEPENDENT);
        }
        let n = derived.len() as u32;
        let parents_or_derived = self.list(&[parents, derived].concat());
        let body = self.list(&body);
        let template = Tree::Template {
            constr: c.constr,
            parents_or_derived,
            self_,
            body,
            derived: (n > 0).then_some(n),
        };
        let rhs = self.ast.derived(c.template, template);
        let Tree::TypeDef { mods: old, .. } = *self.ast.tree(c.cdef) else { unreachable!("a class") };
        let cdef = self.ast.derived(c.cdef, Tree::TypeDef { name: c.name, rhs, mods: old });
        let mods = self.add_impl_use(mods, &c.uses, false);
        self.ast.with_mods(cdef, mods)
    }

    /// Self definition `s` typed as the class unless it has a type.
    fn class_self(&mut self, c: &ClassParts, s: NodeId) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(s) else { return s };
        let tpt = if matches!(self.ast.tree(tpt), Tree::TypeTree) { c.class_type_ref } else { tpt };
        let m = self.ast.modifiers(mods).clone().or(flags::SELF_NAME);
        let s = self.ast.derived(s, Tree::ValDef { name, tpt, rhs, mods });
        self.ast.with_mods(s, m)
    }

    /// The access and `final` of the class, with no written modifiers and annotations.
    fn companion_mods(&self, c: &ClassParts) -> Modifiers {
        let flags = c.mods.flags.and(flags::ACCESS_FLAGS.or(flags::FINAL)).to_common_flags();
        Modifiers { flags, private_within: c.mods.private_within, ..Modifiers::default() }
    }

    /// The expansion of the synthetic companion `object C extends AnyRef derives ... { defs }`.
    fn companion_defs(&mut self, c: &ClassParts, defs: &[NodeId], derived: &[NodeId]) -> Vec<NodeId> {
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
    fn implicit_wrappers(&mut self, c: &mut ClassParts) -> Vec<NodeId> {
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
        let mut paramss = c.constr_vparamss.clone();
        if paramss.first().is_some_and(Vec::is_empty) {
            paramss.remove(0);
        }
        let clauses = self.join_params(&c.constr_tparams, &paramss);
        let creator = self.creator_expr(c);
        let kept = c.mods.flags.to_term_flags().and(flags::GIVEN_OR_IMPLICIT.or(flags::INLINE));
        let m = self.companion_mods(c).or(kept.or(flags::FINAL));
        let mods = self.ast.mods(m);
        let name = TName { is_type: false, ..c.name };
        let wrapper = self.method(name, &clauses, Some(c.class_type_ref), creator, mods);
        vec![self.ast.respan(wrapper, self.ast.span(c.cdef))]
    }

    /// Whether `t` is `AnyVal`, `scala.AnyVal` or `_root_.scala.AnyVal`.
    fn is_any_val(&self, t: NodeId) -> bool {
        let is_text = |n: TName, s: &str| self.text(n) == s;
        match *self.ast.tree(t) {
            Tree::Ident { name } => name.is_type && is_text(name, "AnyVal"),
            Tree::Select { qualifier, name } if name.is_type && is_text(name, "AnyVal") => {
                match *self.ast.tree(qualifier) {
                    Tree::Ident { name } => !name.is_type && is_text(name, "scala"),
                    Tree::Select { qualifier, name } if !name.is_type && is_text(name, "scala") => {
                        matches!(*self.ast.tree(qualifier), Tree::Ident { name } if is_text(name, "_root_"))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}
