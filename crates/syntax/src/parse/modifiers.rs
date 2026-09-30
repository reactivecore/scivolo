use super::Parser;
use super::migration::Migration;
use crate::ast::Span;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::{Mod, ModKind, Modifiers};
use crate::config::Features;
use crate::diag::rewrite_notice;
use crate::token::Token;
use crate::token::sets::{ACCESS_MODIFIER_TOKENS, LOCAL_MODIFIER_TOKENS, TokenSet};

impl Parser<'_> {
    pub fn is_erased(&mut self) -> bool {
        self.is_ident_named("erased")
            && self.s.cfg.features.contains(Features::ERASED_DEFINITIONS)
            && self.s.is_soft_modifier_in_param_modifier_position()
    }

    pub fn is_consume(&self) -> bool {
        self.s.cfg.features.capture_checking() && self.is_ident_named("consume")
    }

    /// Whether the current token is a hard or soft modifier.
    pub fn is_modifier(&self) -> bool {
        crate::token::sets::MODIFIER_TOKENS.contains(self.tok()) || self.s.is_soft_modifier()
    }

    fn mod_of_token(&self, tok: Token, name: &str) -> ModKind {
        match tok {
            Token::Abstract => ModKind::Abstract,
            Token::Final => ModKind::Final,
            Token::Implicit => ModKind::Implicit,
            Token::Given => ModKind::Given,
            Token::Lazy => ModKind::Lazy,
            Token::Override => ModKind::Override,
            Token::Private => ModKind::Private,
            Token::Protected => ModKind::Protected,
            Token::Sealed => ModKind::Sealed,
            _ => match name {
                "inline" => ModKind::Inline,
                "opaque" => ModKind::Opaque,
                "open" => ModKind::Open,
                "transparent" => ModKind::Transparent,
                "infix" => ModKind::Infix,
                "tracked" => ModKind::Tracked,
                "into" => ModKind::Into,
                "erased" => ModKind::Erased,
                _ => ModKind::Update,
            },
        }
    }

    /// `private` with a qualifier drops the flag; `abstract override` becomes `AbsOverride`.
    fn normalize(&mut self, mut mods: Modifiers) -> Modifiers {
        loop {
            if mods.is(flags::PRIVATE) && mods.private_within.is_some() {
                mods.flags = mods.flags.and_not(flags::PRIVATE);
            } else if mods.flags.is_all_of(flags::ABSTRACT.or(flags::OVERRIDE)) {
                mods.flags = mods.flags.and_not(flags::ABSTRACT.or(flags::OVERRIDE));
                mods = self.add_flag(mods, flags::ABS_OVERRIDE);
            } else {
                return mods;
            }
        }
    }

    /// Add the modifier at the current token.
    pub fn add_modifier(&mut self, mods: Modifiers) -> Modifiers {
        if self.is_consume() {
            let start = self.s.skip_token();
            let annot = self.consume_annot();
            let annot = self.at_span1(start, annot);
            let annots: Vec<_> = self.ast.items(mods.annotations).iter().copied().chain([annot]).collect();
            let annotations = self.ast.list(&annots);
            return Modifiers { annotations, ..mods };
        }
        let (tok, name) = (self.tok(), self.s.name_text().to_string());
        let start = self.s.skip_token();
        let kind = self.mod_of_token(tok, &name);
        let span = Span::new(start, self.last_offset(), start);
        let m = Mod { kind, span };
        if mods.is_one_of(kind.flags()) {
            let msg = format!("Repeated modifier {}", kind.flags().flags_string());
            self.syntax_error_span(msg, span);
        }
        self.add_mod(mods, m)
    }

    /// Add `flag`, or report a conflict between term and type flags (dotty's `withAddedFlags`).
    pub fn add_flag(&mut self, mods: Modifiers, flag: FlagSet) -> Modifiers {
        if mods.flags.is_all_of(flag) {
            return mods;
        }
        let f = mods.flags;
        let compatible = f.is_empty()
            || flag.is_empty()
            || f.is_term_flags() && flag.is_term_flags()
            || f.is_type_flags() && flag.is_type_flags();
        if compatible {
            return Modifiers { flags: f.or(flag), ..mods };
        }
        let what = if flag.is_term_flags() { "values" } else { "types" };
        let msg = format!(
            "{} {what} cannot be {}",
            flag.and(flags::MODIFIER_FLAGS).flags_string(),
            f.flags_string()
        );
        let o = self.offset();
        self.s.diags.error(o, msg);
        Modifiers::with_flags(flag)
    }

    /// Add the modifier `m` and its flags.
    pub fn add_mod(&mut self, mods: Modifiers, m: Mod) -> Modifiers {
        let mut mods = self.add_flag(mods, m.kind.flags());
        if !mods.mods.contains(&m) {
            mods.mods.push(m);
        }
        mods
    }

    /// `[id]` or `[this]` after `private` or `protected`.
    fn access_qualifier_opt(&mut self, mods: Modifiers) -> Modifiers {
        if self.tok() != Token::LBracket {
            return mods;
        }
        if mods.is(flags::LOCAL) || mods.private_within.is_some() {
            let o = self.offset();
            self.syntax_error("Duplicate private/protected modifier", o);
        }
        let start = self.offset();
        let mods = self.in_brackets(|p| {
            if p.tok() == Token::This {
                p.next();
                Modifiers { flags: mods.flags.or(flags::LOCAL), ..mods }
            } else {
                let name = p.ident();
                Modifiers { private_within: Some(name.name), ..mods }
            }
        });
        if mods.is(flags::LOCAL) {
            let p = if mods.is(flags::PRIVATE) { "private" } else { "protected" };
            let msg = format!(
                "Ignoring [this] qualifier.\nThe syntax `{p}[this]` will be deprecated in the future; just write `{p}` instead.\nSee: https://docs.scala-lang.org/scala3/reference/dropped-features/this-qualifier.html{}",
                rewrite_notice("This construct", "3.4-migration", "")
            );
            let span = Span::synthetic(start, self.last_offset());
            self.error_or_migration_warning(msg, span, Migration::RemoveThisQualifier);
        }
        mods
    }

    /// `{Modifier}` among `allowed` (and soft modifiers where local modifiers are allowed).
    pub fn modifiers(&mut self, allowed: TokenSet, start: Modifiers) -> Modifiers {
        let mut mods = start;
        loop {
            let soft = self.s.is_soft_modifier()
                && is_subset(LOCAL_MODIFIER_TOKENS, allowed)
                && !matches!(self.s.lookahead().token, Token::ColonOp | Token::ColonFollow | Token::ColonEol);
            if allowed.contains(self.tok()) || soft {
                let access = ACCESS_MODIFIER_TOKENS.contains(self.tok());
                mods = self.add_modifier(mods);
                if access {
                    mods = self.access_qualifier_opt(mods);
                }
            } else if self.s.cur.is_new_line() && (!mods.flags.is_empty() || !mods.annotations.is_empty()) {
                self.next();
            } else {
                return self.normalize(mods);
            }
        }
    }

    /// Annotations then modifiers among `allowed`, of a definition.
    pub fn def_annots_mods(&mut self, allowed: TokenSet) -> Modifiers {
        let start = self.annots_as_mods(true);
        self.modifiers(allowed, start)
    }
}

fn is_subset(a: TokenSet, b: TokenSet) -> bool {
    a.or(b) == b
}
