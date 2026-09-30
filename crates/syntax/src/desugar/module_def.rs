//! Objects as a lazy module value and a module class (dotty's `moduleDef` and
//! `packageModuleDef`).

use super::Desugar;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// The expansion of object definition `mdef`: `<module> val m: m$ = new m$` and
    /// `<module> final class m$ { this: m.type => ... }`; the class keeps the name `m` if it
    /// already names a module class.
    pub fn module_def(&mut self, mdef: NodeId, module_class_name: bool) -> NodeId {
        let Tree::ModuleDef { name, imp, mods } = *self.ast.tree(mdef) else { return mdef };
        let name = self.normalize_name(name, imp);
        let m = self.ast.modifiers(mods).clone();
        self.check_well_formed_module(mdef, &m);
        if m.is(flags::PACKAGE) {
            return self.package_module_def(mdef, name, imp, m);
        }
        if m.flags.is_all_of(flags::ENUM_CASE) {
            return mdef;
        }
        let suffix = if module_class_name { "" } else { "$" };
        let cls_name = self.type_name(&format!("{}{suffix}", self.text(name)));
        let cls_ref = self.ident(cls_name);
        let nu = self.make_new(cls_ref);
        let nu = self.apply(nu, &[]);
        let val_flags = m
            .flags
            .to_term_flags()
            .and(flags::RETAINED_MODULE_VAL_FLAGS)
            .or(flags::MODULE_VAL_CREATION_FLAGS);
        let val_mods = self.ast.mods(Modifiers { flags: val_flags, ..m.clone() });
        let modul = self.ast.node(Tree::ValDef { name, tpt: cls_ref, rhs: Some(nu), mods: val_mods });
        let modul = self.ast.respan(modul, self.ast.span(mdef).start_pos());
        let Tree::Template { constr, parents_or_derived, self_, body, derived } = *self.ast.tree(imp) else {
            return mdef;
        };
        let (self_name, self_rhs, self_mods, self_span) = match self_.map(|s| (s, self.ast.tree(s).clone())) {
            Some((s, Tree::ValDef { name, tpt, rhs, mods })) => {
                if !matches!(self.ast.tree(tpt), Tree::TypeTree) {
                    self.error(s, "objects must not have a self type");
                }
                (name, rhs, self.ast.modifiers(mods).clone(), self.ast.span(s))
            }
            _ => {
                let span = self.ast.span(imp).start_pos();
                (self.term("_"), None, Modifiers::with_flags(flags::PRIVATE_LOCAL), span)
            }
        };
        let reference = self.ident(name);
        let tpt = self.ast.node(Tree::SingletonTypeTree { reference });
        let self_mods = self.ast.mods(self_mods);
        let cls_self = self.ast.node(Tree::ValDef { name: self_name, tpt, rhs: self_rhs, mods: self_mods });
        let cls_self = self.ast.respan(cls_self, self_span);
        let cls_tmpl = Tree::Template { constr, parents_or_derived, self_: Some(cls_self), body, derived };
        let cls_tmpl = self.ast.derived(imp, cls_tmpl);
        let cls_flags = m
            .flags
            .to_type_flags()
            .and(flags::RETAINED_MODULE_CLASS_FLAGS)
            .or(flags::MODULE_CLASS_CREATION_FLAGS);
        let cls_mods = self.ast.mods(Modifiers { flags: cls_flags, ..m });
        let cls = self.ast.node(Tree::TypeDef { name: cls_name, rhs: cls_tmpl, mods: cls_mods });
        if self.ast.attrs(mdef).contains(Attrs::HAS_END_MARKER) {
            self.ast.add_attrs(cls, Attrs::HAS_END_MARKER);
        }
        let cls = self.class_def(cls);
        let cls = self.ast.respan(cls, self.ast.span(mdef));
        let trees = self.list(&[modul, cls]);
        self.ast.node(Tree::Thicket { trees })
    }

    /// `package object p { ... }` as `package p { object package { ... } }`.
    fn package_module_def(&mut self, mdef: NodeId, name: TName, imp: NodeId, m: Modifiers) -> NodeId {
        let span = self.name_span(mdef, name);
        self.check_package_part(name, span);
        let pid = self.ident(name);
        let package = self.term("package");
        let Tree::ModuleDef { mods, .. } = *self.ast.tree(mdef) else { return mdef };
        let object = self.ast.derived(mdef, Tree::ModuleDef { name: package, imp, mods });
        let object = self.ast.with_mods(object, m.and_not(flags::PACKAGE));
        let stats = self.list(&[object]);
        self.ast.node(Tree::PackageDef { pid, stats })
    }

    /// Errors for modifiers an object cannot have (dotty's `checkWellFormedModule`).
    fn check_well_formed_module(&mut self, mdef: NodeId, m: &Modifiers) {
        let not_allowed = |text: &str| format!("Modifier {text} is not allowed for this definition");
        let checks: [(bool, FlagSet, String, bool); 6] = [
            (m.is(flags::OPEN), flags::OPEN, not_allowed("open"), true),
            (m.is(flags::INTO), flags::OPEN, not_allowed("into"), true),
            (m.is(flags::ABSTRACT), flags::ABSTRACT, not_allowed("abstract"), true),
            (m.is(flags::SEALED), flags::SEALED, not_allowed("sealed"), true),
            (
                m.is(flags::FINAL) && !m.is(flags::SYNTHETIC),
                flags::FINAL,
                "Modifier final is redundant for this definition".into(),
                false,
            ),
            (m.is(flags::INFIX), flags::INFIX, not_allowed("infix"), true),
        ];
        for (applies, at, msg, error) in checks {
            if applies {
                self.flag_diagnostic(mdef, m, at, &msg, error);
            }
        }
    }

    /// An error or warning at the written modifier for `flag`, or at `def` without one (dotty's
    /// `flagSourcePos`).
    pub fn flag_diagnostic(&mut self, def: NodeId, m: &Modifiers, flag: FlagSet, msg: &str, error: bool) {
        let span = m.mods.iter().find(|md| md.kind.flags() == flag).map_or(self.ast.span(def), |md| md.span);
        if error {
            self.diags.error_at(span.start(), span.end(), span.point(), msg);
        } else {
            self.diags.warning_at(span.start(), span.end(), span.point(), msg);
        }
    }
}
