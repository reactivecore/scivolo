// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Synthetic parameters `x$n` for clauses given by types alone.

use super::super::Parser;
use super::ParamOwner;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Tree};

impl Parser<'_> {
    /// `FunArgType {, FunArgType}` of a `using` clause without names.
    pub fn context_types(&mut self, owner: ParamOwner, num_lead: usize, implied: &Modifiers) -> Vec<NodeId> {
        let tps = self.comma_separated(|p| p.param_type_of(Self::toplevel_typ));
        self.types_to_params(&tps, owner, num_lead, implied.flags)
    }

    /// Synthetic parameters `x$n` of types `tps`.
    pub fn types_to_params(
        &mut self,
        tps: &[NodeId],
        owner: ParamOwner,
        num_lead: usize,
        implied: FlagSet,
    ) -> Vec<NodeId> {
        let param_flags = if owner.is_class() { flags::LOCAL_PARAM_ACCESSOR } else { flags::PARAM };
        let flags = param_flags.or(flags::SYNTHETIC).or(implied);
        tps.iter()
            .enumerate()
            .map(|(i, &tpt)| {
                let name = self.term(&format!("x${}", num_lead + i + 1));
                let mods = self.mods_with(flags);
                self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
            })
            .collect()
    }
}
