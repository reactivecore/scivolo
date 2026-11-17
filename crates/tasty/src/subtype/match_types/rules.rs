// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Which rules match the cases of a match type: by the source version of the code at hand
//! (dotty's `MatchTypeCaseSpec.analyze` below and from 3.4).

use super::super::Cmp;
use crate::types::TypeId;

/// The rules that match the cases of match types.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MatchTypeRules {
    /// From Scala 3.4: legal patterns only (SIP-56).
    #[default]
    Spec,
    /// Below Scala 3.4: every case with captures by a subtype comparison.
    Legacy,
    /// For code of either version, not known which: both, a reduction where they differ refused.
    Both,
}

impl Cmp<'_> {
    /// The body of the first case of `cases` that `scrut` matches under the universe's rules;
    /// under `Both`, refused (`None`, the reductions exhausted) unless both agree.
    pub fn match_cases_by_rules(&mut self, scrut: TypeId, cases: &[TypeId]) -> Option<TypeId> {
        if self.u.match_type_rules != MatchTypeRules::Both {
            return self.match_cases(scrut, cases);
        }
        self.u.match_type_rules = MatchTypeRules::Spec;
        let spec = self.match_cases(scrut, cases);
        self.u.match_type_rules = MatchTypeRules::Legacy;
        let legacy = self.match_cases(scrut, cases);
        self.u.match_type_rules = MatchTypeRules::Both;
        if spec != legacy {
            self.u.reductions_exhausted = true;
            return None;
        }
        spec
    }
}
