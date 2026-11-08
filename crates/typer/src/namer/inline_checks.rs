//! Definitions outside the subset as far as inlining goes.

use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Whether a definition with the dotty flags `flags` of its modifiers in `owner` is a Scala 2
    /// macro its owner's scope does not hold (dotty's `isScala2MacroInScala3`).
    pub fn is_scala2_macro_in_scala3(&self, owner: SymbolId, flags: FlagSet) -> bool {
        flags.is(f::MACRO) && !flags.is(f::INLINE) && self.u.full_name(owner) != "scala.StringContext"
    }

    /// What about a definition of `kind` with `flags` in `owner` is outside the subset as far as
    /// inlining goes: erased definitions but Scala 2 macros, case classes in an inline method and nested
    /// inline methods.
    pub fn inline_problem(&self, kind: Kind, owner: SymbolId, flags: FlagSet) -> Option<&'static str> {
        let mut outer = Some(owner);
        let mut nested = false;
        while let Some(o) = outer {
            let s = self.u.symbol(o);
            nested |= s.kind == Kind::Def && s.flags.has(tasty::tags::INLINE);
            outer = s.owner;
        }
        let scala2_macro = flags.is(f::MACRO);
        if flags.is(f::ERASED)
            && !scala2_macro
            && (kind != Kind::Val || flags.is_one_of(f::MUTABLE.or(f::LAZY)))
        {
            return Some("erased definition");
        }
        if nested && kind == Kind::Class && flags.is(f::CASE) {
            return Some("case class defined in an inline method");
        }
        if !flags.is(f::INLINE) {
            return None;
        }
        Some(if nested && kind != Kind::Val {
            "inline definition nested in an inline method"
        } else if flags.is_one_of(f::PARAM_ACCESSOR.or(f::TERM_PARAM))
            || kind != Kind::Def && flags.is(f::MUTABLE)
        {
            "inline variable or class parameter"
        } else if kind != Kind::Def && kind != Kind::Val {
            "inline definition"
        } else {
            return None;
        })
    }
}
