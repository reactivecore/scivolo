//! Value definitions: those of pattern variables, and the setters of variables (dotty's
//! `derivedValDef` and `valDef`).

use super::{Desugar, Owner};
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::span::Span;
use crate::ast::tree::{DerivedKind, ModsId};
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Desugar<'_> {
    /// `val named: tpt = rhs` for a pattern variable, at `span` with its point on the name.
    pub fn derived_val_def(
        &mut self,
        span: Span,
        named: NodeId,
        tpt: NodeId,
        rhs: Option<NodeId>,
        mods: ModsId,
    ) -> NodeId {
        let name = TName { is_type: false, ..self.name_of(named) };
        let vdef = self.ast.node(Tree::ValDef { name, tpt, rhs, mods });
        let point = self.ast.span(named).start();
        let vdef = self.ast.respan(vdef, span.with_point(point));
        self.ast.add_attrs(vdef, Attrs::PATTERN_VAR);
        self.val_def(vdef)
    }

    /// `vdef` named, with a setter `v_=` where one is needed.
    pub fn val_def(&mut self, orig: NodeId) -> NodeId {
        let Tree::ValDef { name, tpt, rhs, mods } = *self.ast.tree(orig) else { return orig };
        let name = self.normalize_name(name, tpt);
        let vdef = self.ast.derived(orig, Tree::ValDef { name, tpt, rhs, mods });
        let m = self.ast.modifiers(mods).clone();
        let needs_setter = m.is(flags::MUTABLE)
            && self.owner.is_class()
            && (!m.is(flags::PRIVATE)
                || matches!(
                    self.owner,
                    Owner::Class { is_trait: true, .. } | Owner::Class { is_package_object: true, .. }
                ));
        if !needs_setter {
            return vdef;
        }
        let setter_name = self.term(&format!("{}_=", self.text(name)));
        let param_tpt =
            self.ast.node(Tree::DerivedTypeTree { kind: DerivedKind::SetterParam, watched: name });
        let param_name = self.term("x$1");
        let param_mods = self.mods(flags::SYNTHETIC_TERM_PARAM);
        let param =
            self.ast.node(Tree::ValDef { name: param_name, tpt: param_tpt, rhs: None, mods: param_mods });
        let clause = self.list(&[param]);
        let paramss = self.ast.clauses(&[clause]);
        let unit = self.sym_ref("scala.Unit", true);
        let setter_rhs =
            rhs.filter(|&r| !matches!(self.ast.tree(r), Tree::Empty)).map(|_| self.synthetic_unit());
        let dropped =
            flags::CASE_ACCESSOR.or(flags::GIVEN_OR_IMPLICIT).or(flags::LAZY).or(flags::TRANSPARENT);
        let flags = m.flags.or(flags::ACCESSOR).and_not(dropped);
        let setter_mods = self.ast.mods(Modifiers { flags, ..m });
        let setter =
            Tree::DefDef { name: setter_name, paramss, tpt: unit, rhs: setter_rhs, mods: setter_mods };
        let setter = self.ast.derived(orig, setter);
        self.ast.remove_attrs(setter, Attrs::HAS_END_MARKER);
        let trees = self.list(&[vdef, setter]);
        self.ast.node(Tree::Thicket { trees })
    }
}
