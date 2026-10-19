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

/// `MissingIdent` for identifier `name`, a type if `is_type`, followed by `hint`.
pub fn not_found(name: &str, is_type: bool, hint: &str) -> Message {
    let kind = if is_type { "type " } else { "" };
    message(6, "Not Found", true, format!("Not found: {kind}{name}{hint}"))
}

/// `MissingIdent` for identifier `name`, a type if `is_type`, where only a binding of the other
/// kind has that name, an object if `module`.
pub fn wrong_kind(name: &str, is_type: bool, module: bool) -> Message {
    let text = match is_type {
        true if module => format!("Expected a type, but found a term: {name} - did you mean {name}.type?"),
        true => format!("Expected a type, but found a term: {name}"),
        false => format!("Expected a term, but found a type: {name}"),
    };
    message(6, "Not Found", true, text)
}

/// `TypeMismatch` of a tree of the type shown as `found` with the type shown as `required`, with
/// no note and no addendum.
pub fn type_mismatch(found: &str, required: &str) -> Message {
    message(7, "Type Mismatch", true, format!("Found:    {found}\nRequired: {required}"))
}

/// `MissingImplicitArgument` for `CanEqual` of the types shown as `l` and `r`, worded by its
/// `implicitNotFound` annotation.
pub fn cannot_compare(l: &str, r: &str) -> Message {
    message(172, "Type", false, format!("Values of types {l} and {r} cannot be compared with == or !="))
}

/// `NoMatchingOverload` for the alternatives of `method`, shown with location, of types `types`,
/// none of which matches `args`.
pub fn no_matching_overload(method: &str, types: &[String], args: &str) -> Message {
    let types: String = types.iter().map(|t| format!("\n {t}")).collect();
    let text =
        format!("None of the overloaded alternatives of {method} with types{types}\nmatch arguments {args}");
    message(134, "Type", false, text)
}

/// `AmbiguousOverload` for the alternatives of `method`, shown with location, of types `types`,
/// which all match `args`.
pub fn ambiguous_overload(method: &str, types: &[String], args: &str) -> Message {
    let all = if types.len() == 2 { "both" } else { "all" };
    let types: String = types.iter().map(|t| format!("\n {t}")).collect();
    let text = format!(
        "Ambiguous overload. The overloaded alternatives of {method} with types{types}\n{all} match arguments {args}"
    );
    message(51, "Reference", true, text)
}

/// `WrongNumberOfTypeArgs` for type constructor `tycon` of type parameters `expected` applied to
/// type arguments `actual`.
pub fn wrong_number_of_type_args(tycon: &str, expected: &[String], actual: &[String]) -> Message {
    let prefix = if actual.len() > expected.len() { "Too many" } else { "Not enough" };
    let (expected, actual) = (expected.join(", "), actual.join(", "));
    let text = format!(
        "{prefix} type arguments for {tycon}[{expected}]\nexpected: [{expected}]\nactual:   [{actual}]"
    );
    message(23, "Syntax", true, text)
}

/// `TypeDoesNotTakeParameters` for the type shown as `tpe`, not an F-bounded type parameter.
pub fn type_does_not_take_parameters(tpe: &str) -> Message {
    message(53, "Type", true, format!("{tpe} does not take type parameters"))
}

/// `NotAMember` for member `name`, a type if `is_type`, of a type shown as `site`, followed by
/// `hint`.
pub fn not_a_member(name: &str, is_type: bool, site: &str, hint: &str) -> Message {
    let kind = if is_type { "type" } else { "value" };
    message(8, "Not Found", false, format!("{kind} {name} is not a member of {site}{hint}"))
}

/// The hint of dotty's `didYouMean` for the best name `first` and the other names at its
/// distance, `alts`, sorted.
pub fn did_you_mean(first: &str, alts: &[String]) -> String {
    let mut shown: Vec<&str> = alts.iter().map(String::as_str).filter(|&a| a != first).take(3).collect();
    shown.dedup();
    match shown.is_empty() {
        true => format!(" - did you mean {first}?"),
        false => format!(" - did you mean {first}? or perhaps {}?", shown.join(" or ")),
    }
}
