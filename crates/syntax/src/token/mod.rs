// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Tokens.scala
//! Tokens, numbered as dotty's `Tokens.scala`, and their token sets.

pub mod sets;

/// A token kind; the discriminant is dotty's token number.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Token {
    Empty = 0,
    Error = 1,
    Eof = 2,
    CharLit = 3,
    IntLit = 4,
    DeciLit = 5,
    ExpoLit = 6,
    LongLit = 7,
    FloatLit = 8,
    DoubleLit = 9,
    StringLit = 10,
    StringPart = 11,
    InterpolationId = 12,
    QuoteId = 13,
    Identifier = 14,
    BackquotedIdent = 15,
    If = 20,
    For = 21,
    Else = 22,
    This = 23,
    Null = 24,
    New = 25,
    With = 26,
    Super = 27,
    Case = 28,
    CaseClass = 29,
    CaseObject = 30,
    Val = 31,
    Abstract = 32,
    Final = 33,
    Private = 34,
    Protected = 35,
    Override = 36,
    Implicit = 37,
    Var = 38,
    Def = 39,
    Type = 40,
    Extends = 41,
    True = 42,
    False = 43,
    Object = 44,
    Class = 45,
    Import = 46,
    Package = 47,
    Yield = 48,
    Do = 49,
    Trait = 50,
    Sealed = 51,
    Throw = 52,
    Try = 53,
    Catch = 54,
    Finally = 55,
    While = 56,
    Return = 57,
    Match = 58,
    Lazy = 59,
    Then = 60,
    ForSome = 61,
    Enum = 62,
    Given = 63,
    Export = 64,
    Macro = 65,
    End = 66,
    Comma = 70,
    Semi = 71,
    Dot = 72,
    Uscore = 73,
    ColonOp = 74,
    Equals = 75,
    LArrow = 76,
    Arrow = 77,
    Newline = 78,
    Newlines = 79,
    Subtype = 80,
    Supertype = 81,
    Hash = 82,
    At = 83,
    ViewBound = 84,
    TLArrow = 85,
    CtxArrow = 86,
    Quote = 87,
    ColonFollow = 88,
    ColonEol = 89,
    SelfArrow = 90,
    LParen = 91,
    RParen = 92,
    LBracket = 93,
    RBracket = 94,
    LBrace = 95,
    RBrace = 96,
    Indent = 97,
    Outdent = 98,
    EndLambda = 99,
    XmlStart = 100,
}

impl Token {
    /// The token that closes this opening parenthesis, bracket or brace.
    pub fn closer(self) -> Token {
        match self {
            Token::LParen => Token::RParen,
            Token::LBracket => Token::RBracket,
            Token::LBrace => Token::RBrace,
            Token::Indent => Token::Outdent,
            t => t,
        }
    }

    /// dotty's `tokenString`.
    pub fn show(self) -> &'static str {
        use Token::*;
        match self {
            Empty => "<empty>",
            Error => "erroneous token",
            Eof => "eof",
            CharLit => "character literal",
            IntLit => "integer literal",
            DeciLit => "number literal",
            ExpoLit => "number literal with exponent",
            LongLit => "long literal",
            FloatLit => "float literal",
            DoubleLit => "double literal",
            StringLit | StringPart => "string literal",
            InterpolationId => "string interpolator",
            QuoteId => "quoted identifier",
            Identifier | BackquotedIdent => "identifier",
            CaseClass => "case class",
            CaseObject => "case object",
            Comma => "','",
            Semi => "';'",
            Dot => "'.'",
            Newline | Newlines => "end of statement",
            LParen => "'('",
            RParen => "')'",
            LBracket => "'['",
            RBracket => "']'",
            LBrace => "'{'",
            RBrace => "'}'",
            Indent => "indent",
            Outdent => "unindent",
            EndLambda => "end of single-line lambda",
            XmlStart => "$XMLSTART$<",
            SelfArrow => "=>",
            ColonOp | ColonFollow | ColonEol => ":",
            Quote => "'",
            t => t.keyword().unwrap_or("?"),
        }
    }

    /// dotty's `showToken`: keywords and colons quoted.
    pub fn display(self) -> String {
        let quoted = sets::KEYWORDS.contains(self) || matches!(self, Token::ColonFollow | Token::ColonEol);
        if quoted { format!("'{}'", self.show()) } else { self.show().to_string() }
    }

    /// The source text of a keyword token.
    pub fn keyword(self) -> Option<&'static str> {
        KEYWORDS.iter().find(|(_, t)| *t == self).map(|(s, _)| *s)
    }
}

/// Every keyword the scanner recognizes, with its token, in byte order of the spelling.
const KEYWORDS: [(&str, Token); 57] = [
    ("#", Token::Hash),
    (":", Token::ColonOp),
    ("<%", Token::ViewBound),
    ("<-", Token::LArrow),
    ("<:", Token::Subtype),
    ("=", Token::Equals),
    ("=>", Token::Arrow),
    ("=>>", Token::TLArrow),
    (">:", Token::Supertype),
    ("?=>", Token::CtxArrow),
    ("@", Token::At),
    ("_", Token::Uscore),
    ("abstract", Token::Abstract),
    ("case", Token::Case),
    ("catch", Token::Catch),
    ("class", Token::Class),
    ("def", Token::Def),
    ("do", Token::Do),
    ("else", Token::Else),
    ("end", Token::End),
    ("enum", Token::Enum),
    ("export", Token::Export),
    ("extends", Token::Extends),
    ("false", Token::False),
    ("final", Token::Final),
    ("finally", Token::Finally),
    ("for", Token::For),
    ("forSome", Token::ForSome),
    ("given", Token::Given),
    ("if", Token::If),
    ("implicit", Token::Implicit),
    ("import", Token::Import),
    ("lazy", Token::Lazy),
    ("macro", Token::Macro),
    ("match", Token::Match),
    ("new", Token::New),
    ("null", Token::Null),
    ("object", Token::Object),
    ("override", Token::Override),
    ("package", Token::Package),
    ("private", Token::Private),
    ("protected", Token::Protected),
    ("return", Token::Return),
    ("sealed", Token::Sealed),
    ("super", Token::Super),
    ("then", Token::Then),
    ("this", Token::This),
    ("throw", Token::Throw),
    ("trait", Token::Trait),
    ("true", Token::True),
    ("try", Token::Try),
    ("type", Token::Type),
    ("val", Token::Val),
    ("var", Token::Var),
    ("while", Token::While),
    ("with", Token::With),
    ("yield", Token::Yield),
];

/// The keyword token spelled `text`, if any.
pub fn keyword(text: &str) -> Option<Token> {
    let i = KEYWORDS.binary_search_by(|(s, _)| s.cmp(&text)).ok()?;
    Some(KEYWORDS[i].1)
}

/// The spellings of all keywords, in the order `keyword_at` indexes them.
pub fn keyword_spellings() -> impl Iterator<Item = &'static str> {
    KEYWORDS.iter().map(|(s, _)| *s)
}

/// The keyword token at `index` of `keyword_spellings`, if any.
pub fn keyword_at(index: usize) -> Option<Token> {
    KEYWORDS.get(index).map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::{KEYWORDS, keyword};

    #[test]
    fn keywords_are_sorted_and_found() {
        assert!(KEYWORDS.windows(2).all(|w| w[0].0 < w[1].0));
        for (text, token) in KEYWORDS {
            assert_eq!(keyword(text), Some(token));
            assert_eq!(token.keyword(), Some(text));
        }
        assert_eq!(keyword("foo"), None);
    }
}
