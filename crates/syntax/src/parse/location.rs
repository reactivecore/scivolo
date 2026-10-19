// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
/// Where an expression or pattern is parsed (dotty's `Location`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Location {
    InParens,
    InArgs,
    InColonArg,
    InPattern,
    InGuard,
    InPatternArgs,
    InBlock,
    Elsewhere,
}

impl Location {
    pub fn in_parens(self) -> bool {
        matches!(self, Location::InParens | Location::InArgs)
    }

    pub fn in_pattern(self) -> bool {
        matches!(self, Location::InPattern | Location::InPatternArgs)
    }

    pub fn in_args(self) -> bool {
        matches!(self, Location::InArgs | Location::InColonArg | Location::InPatternArgs)
    }
}

/// What is being parsed where syntax is shared (dotty's `ParseKind`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseKind {
    Expr,
    Type,
    Pattern,
}
