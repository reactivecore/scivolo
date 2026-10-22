//! Definitions outside the subset as far as inlining goes.

use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// What about a definition of `kind` with `flags` in `owner` is outside the subset as far as
    /// inlining goes: macros, erased definitions, case classes in an inline method, nested
    /// inline methods and transparent inline givens.
    pub fn inline_problem(&self, kind: Kind, owner: SymbolId, flags: FlagSet) -> Option<&'static str> {
        let mut outer = Some(owner);
        let mut nested = false;
        while let Some(o) = outer {
            let s = self.u.symbol(o);
            nested |= s.kind == Kind::Def && s.flags.has(tasty::tags::INLINE);
            outer = s.owner;
        }
        if flags.is(f::MACRO) {
            return Some("macro definition");
        }
        if flags.is(f::ERASED) {
            return Some("erased definition");
        }
        if nested && kind == Kind::Class && flags.is(f::CASE) {
            return Some("case class defined in an inline method");
        }
        if !flags.is(f::INLINE) {
            return None;
        }
        Some(if nested {
            "inline definition nested in an inline method"
        } else if flags.is_all_of(f::TRANSPARENT.or(f::GIVEN)) {
            "transparent inline given"
        } else if flags.is_one_of(f::MUTABLE.or(f::PARAM_ACCESSOR).or(f::TERM_PARAM)) {
            "inline variable or class parameter"
        } else if kind != Kind::Def && kind != Kind::Val {
            "inline definition"
        } else {
            return None;
        })
    }
}
