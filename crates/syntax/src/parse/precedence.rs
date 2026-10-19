// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameOps.scala
//   compiler/src/dotty/tools/dotc/parsing/package.scala
//! Precedence and associativity of infix operators.

use crate::scan::is_operator_part;
use unicode_general_category::{GeneralCategory as G, get_general_category};

/// The precedence of infix operator `op` (SLS 6.12.3), -1 for `<error>`.
pub fn precedence(op: &str) -> i32 {
    if op == "<error>" {
        return -1;
    }
    let Some(first) = op.chars().next() else { return 10 };
    let letter = first == '$'
        || first == '_'
        || matches!(
            get_general_category(first),
            G::LowercaseLetter | G::UppercaseLetter | G::OtherLetter | G::TitlecaseLetter | G::LetterNumber
        );
    if letter {
        1
    } else if is_op_assignment(op) {
        0
    } else {
        match first {
            '|' => 2,
            '^' => 3,
            '&' => 4,
            '=' | '!' => 5,
            '<' | '>' => 6,
            ':' => 7,
            '+' | '-' => 8,
            '*' | '/' | '%' => 9,
            _ => 10,
        }
    }
}

/// `+=` and the like, but not `!=`, `<=`, `>=`.
pub fn is_op_assignment(op: &str) -> bool {
    !matches!(op, "!=" | "<=" | ">=" | "")
        && op.ends_with('=')
        && !op.starts_with('=')
        && op.chars().next().is_some_and(is_operator_part)
}

pub fn is_right_assoc(op: &str) -> bool {
    op.ends_with(':')
}
