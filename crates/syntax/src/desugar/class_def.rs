//! A class definition as the class with accessors and synthetic members, its companion
//! object, its implicit wrapper and an enum's lookup methods (the second half of dotty's
//! `classDef`).

use super::Desugar;
use super::class_parts::ClassParts;
use crate::ast::flags;
use crate::ast::{NodeId, TName, Tree};

impl Desugar<'_> {
    /// The expansion of class definition `cdef`.
    pub fn class_def(&mut self, cdef: NodeId) -> NodeId {
        let mut c = self.class_parts(cdef);
        let is_object = c.is(flags::MODULE);
        let is_case_class = c.is_case_class();
        let is_enum = c.is(flags::ENUM) && !c.is(flags::CASE) && !is_object;
        let is_enum_case = c.mods.flags.is_all_of(flags::ENUM_CASE);
        let is_value_class = c.parents.first().is_some_and(|&p| self.is_any_val(p));
        let (case_meths, scaffolding) =
            if is_case_class { self.case_class_meths(&mut c) } else { (Vec::new(), Vec::new()) };
        let mut parents = c.parents.clone();
        if is_enum_case && parents.is_empty() {
            let own_tparams = !c.original_tparams.is_empty();
            parents.push(self.enum_class_type_ref(cdef, own_tparams, &c.constr_tparams));
        }
        if c.is(flags::CASE) && !is_enum_case {
            for name in ["Product", "Serializable"] {
                let name = self.type_name(name);
                parents.push(self.scala_dot(name));
            }
        }
        if is_enum {
            parents.push(self.sym_ref("scala.reflect.Enum", true));
        }
        let (cls_derived, companion_derived) =
            if is_object { (c.derived.clone(), Vec::new()) } else { (Vec::new(), c.derived.clone()) };
        let members = [&c.default_getters[..], &c.enum_cases].concat();
        let companions = if is_case_class {
            let meths = self.case_companion_meths(&mut c);
            self.companion_defs(&c, &[meths, members].concat(), &companion_derived)
        } else if !members.is_empty()
            || !companion_derived.is_empty()
            || is_enum
            || is_value_class && !is_object
        {
            self.companion_defs(&c, &members, &companion_derived)
        } else {
            Vec::new()
        };
        let wrappers = self.implicit_wrappers(&mut c);
        let cdef1 = self.class_with_members(&c, parents, cls_derived, case_meths);
        let cdef1 = self.add_enum_flags(cdef1);
        self.flat_tree(&[&[cdef1][..], &companions, &wrappers, &scaffolding].concat())
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
        for (&d, &implied) in c.derived_tparams.iter().zip(&c.implied_tparams) {
            let m = self.ast.modifiers(self.ast.mods_of(implied).expect("modifiers")).clone();
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

    /// Enum class `cdef` made abstract and sealed, an enum case class final (dotty's
    /// `addEnumFlags`).
    fn add_enum_flags(&mut self, cdef: NodeId) -> NodeId {
        let Some(mods) = self.ast.mods_of(cdef) else { return cdef };
        let m = self.ast.modifiers(mods).clone();
        let added = if m.is(flags::ENUM) && !m.is(flags::CASE) {
            flags::ABSTRACT.or(flags::SEALED)
        } else if m.flags.is_all_of(flags::ENUM_CASE) {
            flags::FINAL
        } else {
            return cdef;
        };
        let span = self.ast.span(cdef);
        let m = self.with_added_flags(m, added, span);
        self.ast.with_mods(cdef, m)
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
