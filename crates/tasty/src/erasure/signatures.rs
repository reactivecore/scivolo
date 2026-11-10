// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Method signatures as members of a class (dotty's `Signature`) and how far two match.

use super::Erased;
use crate::tags::INLINE;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// A part of a signature's parameters: a type-parameter clause's length or an erased parameter.
#[derive(Clone, PartialEq, Debug)]
pub enum ParamSig {
    TypeParams(usize),
    Param(Erased),
}

/// A method's parameter signatures and erased result type; `None` for a member that is not a
/// method (dotty's `NotAMethod`).
pub type Signature = Option<(Vec<ParamSig>, Erased)>;

/// How far two signatures match (dotty's `MatchDegree`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MatchDegree {
    No,
    Params,
    MethodNotAMethod,
    Full,
}

/// dotty's `Signature.matchDegree`.
pub fn match_degree(a: &Signature, b: &Signature) -> MatchDegree {
    fn params(s: &Signature) -> &[ParamSig] {
        s.as_ref().map_or(&[], |(p, _)| p)
    }
    if params(a) != params(b) {
        return MatchDegree::No;
    }
    match (a, b) {
        (None, None) => MatchDegree::Full,
        (Some((_, x)), Some((_, y))) if x == y => MatchDegree::Full,
        (Some(_), None) | (None, Some(_)) => MatchDegree::MethodNotAMethod,
        _ => MatchDegree::Params,
    }
}

impl Universe {
    /// The signature of member `s` as a member of `site`, erased as Scala 2 erases if `scala2`.
    pub fn signature(&mut self, site: TypeId, s: SymbolId, scala2: bool) -> Signature {
        let mut t = self.member_info(site, s);
        if !matches!(self.ty(t), Type::Method { .. } | Type::Poly { .. }) {
            return None;
        }
        let previous = self.set_scala2_erasure(scala2);
        let (mut parts, mut bounds) = (Vec::new(), Vec::new());
        loop {
            match self.ty(t).clone() {
                Type::Poly { binder, params, result } => {
                    let params = self.params_of(params);
                    parts.push(Err(params.len()));
                    bounds.push((binder, params.iter().map(|&(_, b)| b).collect::<Vec<_>>()));
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    parts.extend(self.params_of(params).iter().map(|&(_, p)| Ok(p)));
                    t = result;
                }
                _ => break,
            }
        }
        let mut sig = Vec::new();
        for part in parts {
            match part {
                Err(n) => sig.push(ParamSig::TypeParams(n)),
                Ok(p) if self.is_erased_param(p) => {}
                Ok(p) => {
                    let p = self.upper_bound(p, &bounds);
                    sig.push(ParamSig::Param(self.erase(p)));
                }
            }
        }
        let result = self.upper_bound(t, &bounds);
        let result = self.erase(result);
        self.set_scala2_erasure(previous);
        Some((sig, result))
    }

    /// Whether method `s` is erased as Scala 2 erases: not inline, of a Scala 2 class, not lifted
    /// out of a term.
    pub fn erased_as_scala2(&self, s: SymbolId) -> bool {
        let owner = self.symbol(s).owner;
        let in_scala2_class = owner.is_some_and(|o| self.symbol(o).kind == Kind::Class && self.is_scala2(o));
        in_scala2_class && !self.symbol(s).flags.has(INLINE) && !self.is_lifted(s)
    }
}
