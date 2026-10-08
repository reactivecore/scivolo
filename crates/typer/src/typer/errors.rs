//! The typer's error messages, as dotty's `reporting/messages.scala` words them.

use syntax::diag::{Message, MessageId};

fn type_error(code: u16, explained: bool, text: String) -> Message {
    Message { text, id: Some(MessageId { code, kind: "Type", explained }) }
}

/// `AnonymousFunctionMissingParamType` for parameter `param` of a function written by hand, with
/// nothing inferred and `expected` the expected type of the function, if any.
pub fn missing_parameter_type(param: &str, expected: Option<&str>) -> Message {
    let expected = expected
        .map_or(String::new(), |e| format!("\nExpected type for the whole anonymous function:\n  {e}"));
    let text =
        format!("Missing parameter type\n\nI could not infer the type of the parameter {param}{expected}");
    type_error(81, false, text)
}
