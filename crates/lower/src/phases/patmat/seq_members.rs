// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The members a sequence match selects (dotty's `unapplySeqTypeElemTp`): `apply(Int)` of the
//! element type, `drop(Int)` and `toSeq` conforming to `collection.Seq` of it, and
//! `lengthCompare(Int): Int` or `length: Int`; the length test's member.

use super::Translator;
use tasty::{Kind, SymbolId, Type, TypeId, nme};

/// The members of a sequence-like type a sequence match calls.
pub struct SeqMembers {
    pub apply: SymbolId,
    pub drop: SymbolId,
    pub to_seq: SymbolId,
    /// `lengthCompare(Int)`, or `length` if there is none (dotty's `LengthTest`).
    pub length: SymbolId,
    /// Whether `length` returns an `Int`.
    pub int_length: bool,
}

impl Translator<'_, '_> {
    /// Whether `tpe` is sequence-like.
    pub fn is_sequence(&mut self, tpe: TypeId) -> bool {
        self.seq_members(tpe).is_some()
    }

    /// The members of sequence-like `tpe`.
    pub fn seq_members(&mut self, tpe: TypeId) -> Option<SeqMembers> {
        let (apply, elem) = self.seq_member(tpe, nme::APPLY, |_, _| true)?;
        let seq = self.l.u.class_ref("scala.collection.Seq");
        let seq = self.l.u.intern_applied(seq, &[elem]);
        let conforms = |t: &mut Self, r| t.l.u.is_subtype(r, seq);
        let is_int = |t: &mut Self, r| t.l.u.is_class_named(r, tasty::defn::INT);
        if self.seq_member(tpe, nme::LENGTH_COMPARE, is_int).is_none() {
            self.seq_member(tpe, "length", is_int)?;
        }
        let (length, result) = match self.seq_member(tpe, nme::LENGTH_COMPARE, |_, _| true) {
            Some(found) => found,
            None => self.seq_member(tpe, "length", is_int)?,
        };
        let int_length = is_int(self, result);
        let (drop, _) = self.seq_member(tpe, "drop", conforms)?;
        let (to_seq, _) = self.seq_member(tpe, "toSeq", conforms)?;
        Some(SeqMembers { apply, drop, to_seq, length, int_length })
    }

    /// Method `name` of `tpe` taking one `Int` (dotty's `matchingMember` of `Seq`'s method).
    pub fn seq_method(&mut self, tpe: TypeId, name: &str) -> Option<SymbolId> {
        self.seq_member(tpe, name, |_, _| true).map(|(m, _)| m)
    }

    /// The first method `name` of `tpe`'s class or its bases, taking one `Int` or, for `toSeq`
    /// and `length`, without a parameter list, whose result as seen from `tpe` satisfies `fits`;
    /// with that result.
    fn seq_member(
        &mut self,
        tpe: TypeId,
        name: &str,
        fits: impl Fn(&mut Self, TypeId) -> bool,
    ) -> Option<(SymbolId, TypeId)> {
        let class = self.l.class_of(tpe)?;
        let int = self.l.u.class_ref(tasty::defn::INT);
        let int = self.l.u.class_symbol(int);
        for base in self.l.u.linearization(class) {
            for m in self.l.u.decls_named(base, name) {
                if self.l.u.symbol(m).kind != Kind::Def {
                    continue;
                }
                let info = self.l.u.member_info(tpe, m);
                let (params, result) = match *self.l.u.ty(info) {
                    Type::Method { params, result, .. } => (Some(params), result),
                    Type::Poly { .. } => continue,
                    Type::ByName(result) => (None, result),
                    _ => (None, info),
                };
                if matches!(self.l.u.ty(result), Type::Method { .. } | Type::Poly { .. }) {
                    continue;
                }
                let shape = match name {
                    "toSeq" | "length" => params.is_none(),
                    _ => params.is_some_and(|p| {
                        let p = self.l.u.params_of(p);
                        p.len() == 1 && self.l.u.class_symbol(p[0].1) == int
                    }),
                };
                if shape && fits(self, result) {
                    return Some((m, result));
                }
            }
        }
        None
    }
}
