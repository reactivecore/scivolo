// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The flags of symbols: the bits of the TASTy modifier tags, with bits 2 to 5 for `Method`,
//! `Package`, `Param` and `AbsOverride`, bit 50 for `JavaDefined`, bit 51 for `Scala2x` and bit
//! 53 for `Deferred`.

use super::symbols::METHOD;
use crate::host::Host;
use tasty::tags::{ABSTRACT, FINAL, LAZY, OBJECT, OPAQUE, OVERRIDE, PROTECTED, PROTECTEDqualified};
use tasty::{Kind, SymbolId, Type};

const PACKAGE: u32 = 3;
const PARAM: u32 = 4;
const ABS_OVERRIDE: u32 = 5;
const JAVA_DEFINED: u32 = 50;
const SCALA2X: u32 = 51;
const DEFERRED: u32 = 53;

impl Host {
    /// The flags of `s` as dotty's `TreeUnpickler` reads them (`readModifiers`, `normalizeFlags`),
    /// but for `ParamAccessor`, `NoInits` and `StableRealizable`.
    pub fn flags(&mut self, s: SymbolId) -> u64 {
        let deferred = if self.is_deferred(s) { 1 << DEFERRED } else { 0 };
        let sym = self.u.symbol(s);
        let has = |tag: u8| sym.flags.has(tag);
        let bit = |tag: u8| 1u64 << tag;
        let mut bits = sym.flags.0 & !0b11;
        if has(PROTECTEDqualified) {
            bits |= bit(PROTECTED);
        }
        if sym.kind.is_term() && has(ABSTRACT) && has(OVERRIDE) {
            bits = bits & !bit(ABSTRACT) | 1 << ABS_OVERRIDE;
        }
        if has(OBJECT) {
            bits |= bit(FINAL) | if sym.kind.is_term() { bit(LAZY) } else { 0 };
        }
        let in_class = sym.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        bits | deferred
            | match sym.kind {
                Kind::Class if self.u.is_scala2(s) => 1 << SCALA2X,
                Kind::Def => 1 << METHOD,
                Kind::Package => 1 << PACKAGE | 1 << JAVA_DEFINED | bit(OBJECT) | bit(FINAL),
                Kind::Param if in_class => 0,
                Kind::Param | Kind::TypeParam => 1 << PARAM,
                _ => 0,
            }
    }

    /// Whether `s` is a value or method member of a class without a definition, but for a
    /// constructor, an abstract type member or an opaque type alias.
    pub fn is_deferred(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        let (kind, tasty, opaque) = (sym.kind, sym.tree.is_some(), sym.flags.has(OPAQUE));
        if !sym.owner.is_some_and(|o| self.u.is_class(o)) || self.u.is_constructor(s) {
            return false;
        }
        match kind {
            Kind::Val | Kind::Def if tasty => self.u.body(s).is_none(),
            Kind::Val | Kind::Def => self.u.is_deferred(s),
            Kind::Type => {
                let mut info = self.u.info(s);
                if let Type::Lambda { result, .. } = self.u.types().get(info) {
                    info = *result;
                }
                opaque || matches!(self.u.types().get(info), Type::Bounds(..))
            }
            _ => false,
        }
    }
}
