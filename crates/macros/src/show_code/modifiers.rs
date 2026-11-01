// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala

use super::Code;
use tasty::SymbolId;
use tasty::tags::{
    CONTRAVARIANT, COVARIANT, FINAL, IMPLICIT, INLINE, LOCAL, OBJECT, OVERRIDE, PRIVATE, PROTECTED,
    PROTECTEDqualified,
};

impl Code<'_> {
    /// The modifiers of definition `s` before `val`, `var` or `def`.
    pub fn modifiers(&mut self, s: Option<SymbolId>, def: bool) {
        if self.has(s, IMPLICIT) {
            self.keyword("implicit ");
        }
        if def && self.has(s, INLINE) {
            self.keyword("inline ");
        }
        if self.has(s, OVERRIDE) {
            self.keyword("override ");
        }
        if self.has(s, FINAL) && !self.has(s, OBJECT) {
            self.keyword("final ");
        }
        self.protected_or_private(s);
    }

    /// The access modifier of `s` with a space, if it has one (dotty's `printProtectedOrPrivate`).
    pub fn protected_or_private(&mut self, s: Option<SymbolId>) -> bool {
        let within = s.and_then(|s| self.h.u.access_qualifier(s));
        let access = if self.has(s, PROTECTED) || self.has(s, PROTECTEDqualified) {
            "protected"
        } else if within.is_some() || self.has(s, PRIVATE) {
            "private"
        } else {
            return false;
        };
        self.keyword(access);
        match within.map(|q| self.h.u.symbol(q).name.clone()) {
            _ if self.has(s, LOCAL) => self.push("[this]"),
            Some(q) => self.push(&format!("[{q}]")),
            None => {}
        }
        self.push(" ");
        true
    }

    pub fn variance(&mut self, s: Option<SymbolId>) {
        if self.has(s, COVARIANT) {
            self.definition_name("+");
        } else if self.has(s, CONTRAVARIANT) {
            self.definition_name("-");
        }
    }
}
