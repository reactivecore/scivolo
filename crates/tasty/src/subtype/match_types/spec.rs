//! The cases of a match type as SIP-56 patterns (dotty's `MatchTypeCaseSpec.analyze` and
//! `tryConvertToSpecPattern`).

use super::super::Cmp;
use crate::types::{Binder, Type, TypeId};
use crate::{Kind, SymbolId};

/// A legal pattern (dotty's `MatchTypeCasePattern`).
pub enum Pattern {
    /// A capture by index, and whether it is a wildcard.
    Capture(u32, bool),
    TypeTest,
    BaseTypeTest {
        class: SymbolId,
        args: Vec<Pattern>,
        needs_concrete: bool,
    },
    CompileTimeS(Box<Pattern>),
}

impl Pattern {
    fn needs_concrete_in_variant_pos(&self) -> bool {
        match self {
            Pattern::Capture(_, wildcard) => !wildcard,
            Pattern::TypeTest => false,
            _ => true,
        }
    }
}

/// A case of a match type.
pub enum Spec {
    /// A case without captures: its pattern and body.
    SubTypeTest(TypeId, TypeId),
    /// A case with captures and a legal pattern.
    Specced(Specced),
    /// A case outside the patterns reduced here: abstract type constructors, type member
    /// extractors, illegal patterns, missing captures.
    Unsupported,
}

pub struct Specced {
    pub binder: Binder,
    pub names: Vec<String>,
    /// The bounds of each capture.
    pub bounds: Vec<TypeId>,
    pub pattern: Pattern,
    pub pat: TypeId,
    pub body: TypeId,
}

impl Cmp<'_> {
    pub fn analyze(&mut self, case: TypeId) -> Spec {
        match self.u.types().get(case).clone() {
            Type::MatchCase(pat, body) => Spec::SubTypeTest(pat, body),
            Type::Lambda { binder, params, result } => {
                let Type::MatchCase(pat, body) = *self.u.types().get(result) else {
                    return Spec::Unsupported;
                };
                let mut unaccounted = vec![true; params.len()];
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                if self.missing_captures(pat, binder, params.len()) {
                    return Spec::Unsupported;
                }
                let Some(pattern) = self.spec_pattern(pat, 1, (binder, &names), &mut unaccounted) else {
                    return Spec::Unsupported;
                };
                if unaccounted.contains(&true) {
                    return Spec::Unsupported;
                }
                let bounds = params.iter().map(|&(_, b)| b).collect();
                Spec::Specced(Specced { binder, names, bounds, pattern, pat, body })
            }
            _ => Spec::Unsupported,
        }
    }

    /// Whether pattern `pat` misses one of the `n` captures of `binder`.
    fn missing_captures(&mut self, pat: TypeId, binder: Binder, n: usize) -> bool {
        let mut present = vec![false; n];
        self.u.map_type(pat, &mut |u, x| {
            if let Type::Param(b, i) = *u.types().get(x)
                && b == binder
                && let Some(p) = present.get_mut(i as usize)
            {
                *p = true;
            }
            None
        });
        present.contains(&false)
    }

    /// `pat` as a legal pattern at `variance`; `None` for a pattern outside them.
    fn spec_pattern(
        &mut self,
        pat: TypeId,
        variance: i8,
        captures: (Binder, &[String]),
        unaccounted: &mut [bool],
    ) -> Option<Pattern> {
        match self.u.types().get(pat).clone() {
            Type::Param(b, i) if b == captures.0 => {
                *unaccounted.get_mut(i as usize)? = false;
                let name = captures.1.get(i as usize)?;
                Some(Pattern::Capture(i, name == "_" || name.starts_with("_$")))
            }
            Type::Applied(tycon, args) if variance == 1 => {
                if matches!(self.u.types().get(tycon), Type::Param(b, _) if *b == captures.0) {
                    return None;
                }
                if !matches!(self.u.types().get(tycon), Type::Sym(..) | Type::TypeRef(..)) {
                    return Some(Pattern::TypeTest);
                }
                let s = self.named(tycon)?.sym?;
                if self.u.symbol(s).kind == Kind::Class {
                    if let Some(pairs) = self.nested_pairs(s, &args) {
                        return self.spec_pattern(pairs, variance, captures, unaccounted);
                    }
                    let variances = self.u.variances(s);
                    if variances.len() != args.len() {
                        return None;
                    }
                    let mut pats = Vec::new();
                    for (&a, &v) in args.iter().zip(&variances) {
                        pats.push(self.spec_pattern(a, v, captures, unaccounted)?);
                    }
                    if pats.iter().all(|p| matches!(p, Pattern::TypeTest)) {
                        return Some(Pattern::TypeTest);
                    }
                    let needs_concrete = pats
                        .iter()
                        .zip(&variances)
                        .any(|(p, &v)| v != 0 && p.needs_concrete_in_variant_pos());
                    return Some(Pattern::BaseTypeTest { class: s, args: pats, needs_concrete });
                }
                if self.is_succ(tycon) && args.len() == 1 {
                    return match self.spec_pattern(args[0], variance, captures, unaccounted)? {
                        Pattern::TypeTest => Some(Pattern::TypeTest),
                        p => Some(Pattern::CompileTimeS(Box::new(p))),
                    };
                }
                None
            }
            Type::Refined(_, _, info) if variance == 1 => match *self.u.types().get(info) {
                Type::Alias(a) if matches!(self.u.types().get(a), Type::Param(b, _) if *b == captures.0) => {
                    None
                }
                _ => Some(Pattern::TypeTest),
            },
            _ => Some(Pattern::TypeTest),
        }
    }
}
