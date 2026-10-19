// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Tokens.scala
//! dotty's token sets (`Tokens.scala`), as 128-bit sets.

use super::Token::{self, *};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TokenSet(u128);

impl TokenSet {
    pub const fn of(tokens: &[Token]) -> TokenSet {
        let mut bits = 0u128;
        let mut i = 0;
        while i < tokens.len() {
            bits |= 1 << tokens[i] as u8;
            i += 1;
        }
        TokenSet(bits)
    }

    /// The tokens numbered `lo..=hi`.
    pub const fn range(lo: Token, hi: Token) -> TokenSet {
        let (lo, hi) = (lo as u8, hi as u8);
        TokenSet(((1u128 << (hi - lo + 1)) - 1) << lo)
    }

    pub const fn or(self, other: TokenSet) -> TokenSet {
        TokenSet(self.0 | other.0)
    }

    pub const fn contains(self, t: Token) -> bool {
        self.0 & (1 << t as u8) != 0
    }
}

pub const IDENTIFIER_TOKENS: TokenSet = TokenSet::of(&[Identifier, BackquotedIdent]);
pub const ALPHA_KEYWORDS: TokenSet = TokenSet::range(If, End);
pub const SYMBOLIC_KEYWORDS: TokenSet = TokenSet::range(Uscore, CtxArrow);
pub const KEYWORDS: TokenSet = ALPHA_KEYWORDS.or(SYMBOLIC_KEYWORDS);
pub const SIMPLE_LITERAL_TOKENS: TokenSet =
    TokenSet::range(CharLit, StringLit).or(TokenSet::of(&[True, False]));
pub const LITERAL_TOKENS: TokenSet =
    SIMPLE_LITERAL_TOKENS.or(TokenSet::of(&[InterpolationId, QuoteId, Null]));
pub const ATOMIC_EXPR_TOKENS: TokenSet = LITERAL_TOKENS
    .or(IDENTIFIER_TOKENS)
    .or(TokenSet::of(&[Uscore, Null, This, Super, True, False, Return, QuoteId, XmlStart]));
pub const OPEN_PARENS_TOKENS: TokenSet = TokenSet::of(&[LBrace, LParen, LBracket]);
pub const CAN_START_INFIX_EXPR_TOKENS: TokenSet =
    ATOMIC_EXPR_TOKENS.or(OPEN_PARENS_TOKENS).or(TokenSet::of(&[Quote, New]));
pub const CAN_START_EXPR_TOKENS3: TokenSet =
    CAN_START_INFIX_EXPR_TOKENS.or(TokenSet::of(&[Indent, If, While, For, Try, Throw]));
pub const CAN_START_EXPR_TOKENS2: TokenSet = CAN_START_EXPR_TOKENS3.or(TokenSet::of(&[Do]));
pub const CAN_START_INFIX_TYPE_TOKENS: TokenSet =
    LITERAL_TOKENS.or(IDENTIFIER_TOKENS).or(TokenSet::of(&[This, Super, Uscore, LParen, LBrace, At]));
pub const CAN_START_TYPE_TOKENS: TokenSet = CAN_START_INFIX_TYPE_TOKENS.or(TokenSet::of(&[LBrace]));
pub const CAN_START_PATTERN_TOKENS: TokenSet =
    ATOMIC_EXPR_TOKENS.or(OPEN_PARENS_TOKENS).or(TokenSet::of(&[Uscore, Quote]));
pub const CAN_FOLLOW_SIMPLE_TYPE_TOKENS: TokenSet = TokenSet::of(&[
    At,
    With,
    ColonOp,
    ColonFollow,
    ColonEol,
    LBrace,
    Identifier,
    BackquotedIdent,
    Arrow,
    CtxArrow,
    Match,
    ForSome,
]);
pub const TEMPLATE_INTRO_TOKENS: TokenSet =
    TokenSet::of(&[Class, Trait, Object, Enum, CaseClass, CaseObject]);
pub const DCL_INTRO_TOKENS: TokenSet = TokenSet::of(&[Def, Val, Var, Type, Given]);
pub const DEF_INTRO_TOKENS: TokenSet = TEMPLATE_INTRO_TOKENS.or(DCL_INTRO_TOKENS);
pub const LOCAL_MODIFIER_TOKENS: TokenSet = TokenSet::of(&[Abstract, Final, Sealed, Implicit, Lazy]);
pub const ACCESS_MODIFIER_TOKENS: TokenSet = TokenSet::of(&[Private, Protected]);
pub const MODIFIER_TOKENS: TokenSet =
    LOCAL_MODIFIER_TOKENS.or(ACCESS_MODIFIER_TOKENS).or(TokenSet::of(&[Override]));
pub const MODIFIER_TOKENS_OR_CASE: TokenSet = MODIFIER_TOKENS.or(TokenSet::of(&[Case]));
pub const MODIFIER_FOLLOWERS: TokenSet = MODIFIER_TOKENS.or(DEF_INTRO_TOKENS);
pub const MUST_START_STAT_TOKENS: TokenSet =
    DEF_INTRO_TOKENS.or(MODIFIER_TOKENS).or(TokenSet::of(&[Import, Export, Package]));
pub const CAN_START_STAT_TOKENS2: TokenSet =
    CAN_START_EXPR_TOKENS2.or(MUST_START_STAT_TOKENS).or(TokenSet::of(&[At, Case, End]));
pub const CAN_START_STAT_TOKENS3: TokenSet =
    CAN_START_EXPR_TOKENS3.or(MUST_START_STAT_TOKENS).or(TokenSet::of(&[At, Case, End]));
pub const CAN_END_STAT_TOKENS: TokenSet =
    ATOMIC_EXPR_TOKENS.or(TokenSet::of(&[Type, Given, RParen, RBrace, RBracket, Outdent, EndLambda]));
pub const STOP_SCAN_TOKENS: TokenSet = MUST_START_STAT_TOKENS.or(TokenSet::of(&[
    If, Else, While, Do, For, Yield, New, Try, Catch, Finally, Throw, Return, Match, Semi, Eof,
]));
pub const NUMERIC_LIT_TOKENS: TokenSet =
    TokenSet::of(&[IntLit, DeciLit, ExpoLit, LongLit, FloatLit, DoubleLit]);
pub const STAT_CTD_TOKENS: TokenSet = TokenSet::of(&[Then, Else, Do, Catch, Finally, Yield, Match]);
pub const CLOSING_REGION_TOKENS: TokenSet =
    TokenSet::of(&[RBrace, RParen, RBracket, Case]).or(STAT_CTD_TOKENS);
pub const ACCEPT_OUTDENT_TOKENS: TokenSet = TokenSet::of(&[RBrace, RParen, RBracket]).or(STAT_CTD_TOKENS);
pub const CAN_START_INDENT_TOKENS: TokenSet = STAT_CTD_TOKENS
    .or(TokenSet::of(&[ColonEol, With, Equals, Arrow, CtxArrow, LArrow, While, Try, For, If, Throw, Return]));
pub const START_PARAM_TOKENS: TokenSet = MODIFIER_TOKENS.or(TokenSet::of(&[Val, Var, At]));
pub const SCALA3_KEYWORDS: TokenSet = TokenSet::of(&[Enum, Given]);
pub const END_MARKER_TOKENS: TokenSet =
    IDENTIFIER_TOKENS.or(TokenSet::of(&[If, While, For, Match, Try, New, Throw, Given, Val, This]));
pub const COLON_EOL_PREDECESSORS: TokenSet =
    TokenSet::of(&[RParen, RBracket, BackquotedIdent, This, Super, New]);
pub const CLOSING_PARENS: TokenSet = TokenSet::of(&[RParen, RBracket, RBrace]);
pub const CAN_START_CAPTURE_SET_CONTENTS_TOKENS: TokenSet =
    TokenSet::of(&[Identifier, BackquotedIdent, This, RBrace]);

/// The soft modifiers that need no language import.
pub const SOFT_MODIFIER_NAMES: [&str; 6] = ["inline", "into", "opaque", "open", "transparent", "infix"];
