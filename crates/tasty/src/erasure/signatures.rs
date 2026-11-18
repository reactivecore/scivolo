// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Method signatures as members of a class (dotty's `Signature`) and how far two match.

use super::Erased;
use crate::tags::INLINE;
use crate::types::{Binder, NameId, Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// The binder and parameters of a value parameter clause.
pub type ParamClause = (Binder, Vec<(NameId, TypeId)>);

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
        let info = self.member_info(site, s);
        self.info_signature(info, scala2)
    }

    /// Computes the signature of method type `t`, erased as Scala 2 erases if `scala2`; `None` for
    /// any other type.
    pub fn info_signature(&mut self, mut t: TypeId, scala2: bool) -> Signature {
        if !matches!(self.ty(t), Type::Method { .. } | Type::Poly { .. }) {
            return None;
        }
        let previous = self.set_scala2_erasure(scala2);
        let (mut parts, mut bounds, mut clauses) = (Vec::new(), Vec::new(), Vec::new());
        loop {
            match *self.ty(t) {
                Type::Poly { binder, params, result } => {
                    let params = self.params_of(params);
                    parts.push(Err(params.len()));
                    bounds.push((binder, params.iter().map(|&(_, b)| b).collect::<Vec<_>>()));
                    t = result;
                }
                Type::Method { binder, params, result, .. } => {
                    let params = self.params_of(params).to_vec();
                    parts.extend(params.iter().map(|&(_, p)| Ok(p)));
                    clauses.push((binder, params));
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
                    let p = self.singleton_param_type(p, &clauses);
                    let p = self.upper_bound(p, &bounds);
                    sig.push(ParamSig::Param(self.erase(p)));
                }
            }
        }
        let result = self.singleton_param_type(t, &clauses);
        let result = self.upper_bound(result, &bounds);
        let result = self.erase(result);
        self.set_scala2_erasure(previous);
        Some((sig, result))
    }

    /// Determines the type that `t` has in a method type with `clauses`: for `x.type` of a
    /// parameter `x` of a clause, the type of `x` there, else `t`.
    pub fn singleton_param_type(&mut self, mut t: TypeId, clauses: &[ParamClause]) -> TypeId {
        for _ in clauses.iter().flat_map(|(_, params)| params) {
            let Some(info) = self.singleton_param_info(t, clauses) else { break };
            t = info;
        }
        t
    }

    /// Determines the declared type of parameter `x` of `clauses` for `t`, `x.type`, its
    /// references to parameters of `clauses` by binder; `None` for another type.
    pub fn singleton_param_info(&mut self, t: TypeId, clauses: &[ParamClause]) -> Option<TypeId> {
        let as_refs = |u: &mut Self, t| clauses.iter().fold(t, |r, (b, ps)| u.param_syms_as_refs(r, ps, *b));
        let as_ref = as_refs(self, t);
        let (binder, i) = self.param_ref(as_ref)?;
        let (_, params) = clauses.iter().find(|(b, _)| *b == binder)?;
        let &(_, param) = params.get(i as usize)?;
        Some(as_refs(self, param))
    }

    /// Computes the signature of member `s` of `site` with the aliases `site` gives to the types
    /// in its info followed, as `Repr[Int]` for `String` in a class with `type Repr[X] = String`.
    pub fn dealiased_signature(&mut self, site: TypeId, s: SymbolId, scala2: bool) -> Signature {
        let info = self.member_info(site, s);
        let info = self.map_type(info, &mut |u, x| {
            if !matches!(u.ty(x), Type::Applied(..) | Type::Sym(Some(_), _)) {
                return None;
            }
            Some(u.normalize(x)).filter(|&n| n != x)
        });
        self.info_signature(info, scala2)
    }

    /// Whether method `s` is erased as Scala 2 erases: not inline, of a Scala 2 class, not lifted
    /// out of a term.
    pub fn erased_as_scala2(&self, s: SymbolId) -> bool {
        let owner = self.symbol(s).owner;
        let in_scala2_class = owner.is_some_and(|o| self.symbol(o).kind == Kind::Class && self.is_scala2(o));
        in_scala2_class && !self.symbol(s).flags.has(INLINE) && !self.is_lifted(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{Erased, MatchDegree, ParamSig, Signature, match_degree};

    #[test]
    fn match_degrees_as_in_dotty() {
        let int = || ParamSig::Param(Erased::Prim('i'));
        let method = |params: Vec<ParamSig>, result: Erased| -> Signature { Some((params, result)) };
        let f_int = method(vec![int()], Erased::Unit);
        let f_int_object = method(vec![int()], Erased::object());
        let f_long = method(vec![ParamSig::Param(Erased::Prim('j'))], Erased::Unit);
        let poly = method(vec![ParamSig::TypeParams(1), int()], Erased::Unit);
        let nullary = method(vec![], Erased::Unit);
        let cases = [
            (&f_int, &f_int, MatchDegree::Full),
            (&f_int, &f_int_object, MatchDegree::Params),
            (&f_int, &f_long, MatchDegree::No),
            (&f_int, &poly, MatchDegree::No),
            (&nullary, &None, MatchDegree::MethodNotAMethod),
            (&None, &nullary, MatchDegree::MethodNotAMethod),
            (&f_int, &None, MatchDegree::No),
            (&None, &None, MatchDegree::Full),
        ];
        for (a, b, expected) in cases {
            assert_eq!(match_degree(a, b), expected, "{a:?} and {b:?}");
        }
    }
}
