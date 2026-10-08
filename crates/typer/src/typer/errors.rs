//! The typer's error messages, as dotty's `reporting/messages.scala` words them.

use syntax::diag::{Message, MessageId};

fn message(code: u16, kind: &'static str, explained: bool, text: String) -> Message {
    Message { text, id: Some(MessageId { code, kind, explained }) }
}

/// `AnonymousFunctionMissingParamType` for parameter `param` of a function written by hand, with
/// nothing inferred and `expected` the expected type of the function, if any.
pub fn missing_parameter_type(param: &str, expected: Option<&str>) -> Message {
    let expected = expected
        .map_or(String::new(), |e| format!("\nExpected type for the whole anonymous function:\n  {e}"));
    let text =
        format!("Missing parameter type\n\nI could not infer the type of the parameter {param}{expected}");
    message(81, "Type", false, text)
}

/// `MissingIdent` for identifier `name`, a type if `is_type`, with no name to suggest.
pub fn not_found(name: &str, is_type: bool) -> Message {
    let kind = if is_type { "type " } else { "" };
    message(6, "Not Found", true, format!("Not found: {kind}{name}"))
}
