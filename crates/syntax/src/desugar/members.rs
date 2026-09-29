//! Checks and names of member definitions, and type members (dotty's `checkModifiers`,
//! `normalizeName` and `typeDef`).

use super::{Desugar, Owner};
use crate::ast::flags::{self, FlagSet};
use crate::ast::{NodeId, TName, Tree};

impl Desugar<'_> {
    /// `name`, or for an anonymous given the name invented from `impl`.
    pub fn normalize_name(&mut self, name: TName, impl_: NodeId) -> TName {
        if !self.text(name).is_empty() {
            return name;
        }
        TName { is_type: name.is_type, ..self.invent_given_name(impl_) }
    }

    /// Definition `id` without the modifiers `opaque` and `tracked` where they are not allowed,
    /// with an error for each.
    pub fn check_modifiers(&mut self, id: NodeId) -> NodeId {
        if !matches!(
            self.ast.tree(id),
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ModuleDef { .. }
        ) {
            return id;
        }
        let mut tested = id;
        if let Tree::TypeDef { rhs, .. } = *self.ast.tree(id)
            && self.flags_of(id).is(flags::OPAQUE)
            && self.lacks_alias(rhs)
        {
            self.error(id, "opaque type must have a right-hand side");
            tested = self.without_flags(tested, flags::OPAQUE);
        }
        for flag in [flags::OPAQUE, flags::TRACKED] {
            if self.flags_of(tested).is(flag) && !self.allows(id, flag) {
                self.error(
                    id,
                    &format!("Modifier {} is not allowed for this definition", flag.flags_string()),
                );
                tested = self.without_flags(tested, flag);
            }
        }
        tested
    }

    fn flags_of(&self, id: NodeId) -> FlagSet {
        self.ast.mods_of(id).map_or(flags::EMPTY, |m| self.ast.modifiers(m).flags)
    }

    fn without_flags(&mut self, id: NodeId, flags: FlagSet) -> NodeId {
        let Some(m) = self.ast.mods_of(id) else { return id };
        let m = self.ast.modifiers(m).clone().without_flags(flags);
        self.ast.with_mods(id, m)
    }

    /// Whether the type definition rhs `rhs` is bounds without an alias.
    fn lacks_alias(&self, rhs: NodeId) -> bool {
        match *self.ast.tree(rhs) {
            Tree::TypeBoundsTree { alias, .. } => alias.is_none(),
            Tree::LambdaTypeTree { body, .. } => self.lacks_alias(body),
            _ => false,
        }
    }

    /// Whether definition `id` may have modifier `flag`.
    fn allows(&self, id: NodeId, flag: FlagSet) -> bool {
        match *self.ast.tree(id) {
            Tree::TypeDef { rhs, .. } if flag == flags::OPAQUE => self.opaque_rhs_ok(rhs),
            Tree::ValDef { mods, .. } if flag == flags::TRACKED => {
                self.owner != Owner::Term { is_constructor: false }
                    || self.ast.modifiers(mods).is_one_of(flags::PARAM.or(flags::PARAM_ACCESSOR))
            }
            _ => false,
        }
    }

    fn opaque_rhs_ok(&self, rhs: NodeId) -> bool {
        match *self.ast.tree(rhs) {
            Tree::TypeBoundsTree { alias, .. } => alias.is_some(),
            Tree::Template { .. } | Tree::MatchTypeTree { .. } => false,
            Tree::LambdaTypeTree { body, .. } => self.opaque_rhs_ok(body),
            _ => true,
        }
    }

    /// Type member `id` with its context bounds as deferred givens, and an error for `into`.
    pub fn type_def(&mut self, id: NodeId) -> NodeId {
        let Tree::TypeDef { mods, .. } = *self.ast.tree(id) else { return id };
        let m = self.ast.modifiers(mods).clone();
        let evidence_flags =
            m.flags.to_term_flags().and(flags::ACCESS_FLAGS).or(flags::LAZY).or(flags::DEFERRED_GIVEN_FLAGS);
        let mut evidence = Vec::new();
        let result = self.desugar_context_bounds(id, &mut evidence, evidence_flags, None, &[]);
        if m.is(flags::INTO) && !m.is(flags::OPAQUE) {
            let at = m.mods.iter().find(|md| md.kind.flags() == flags::INTO).map(|md| md.span);
            match at {
                Some(span) => self.diags.error_at(
                    span.start(),
                    span.end(),
                    span.point(),
                    "Modifier into is not allowed for this definition",
                ),
                None => self.error(id, "Modifier into is not allowed for this definition"),
            }
        }
        if evidence.is_empty() {
            return result;
        }
        let trees = self.list(&[&[result][..], &evidence].concat());
        self.ast.node(Tree::Thicket { trees })
    }
}
