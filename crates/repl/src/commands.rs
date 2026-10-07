//! The REPL's commands: `:` and a letter, then an argument (dotty's `ParseResult.commands`).

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Reset(String),
    TypeOf(String),
    Unknown(String),
}

const COMMANDS: [&str; 5] = [":quit", ":exit", ":help", ":reset", ":type"];

pub const HELP: &str = "The REPL has several commands available:

:help                    print this summary
:quit                    exit the interpreter
:type <expression>       evaluate the type of the given expression
:reset                   reset the repl to its initial state, forgetting all session entries";

/// The command `text` is, if it starts with `:` and a letter; a prefix names the one command it
/// starts.
pub fn parse(text: &str) -> Option<Command> {
    let text = text.trim();
    let rest = text.strip_prefix(':')?;
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let (name, arg) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let matching: Vec<&str> = COMMANDS.into_iter().filter(|c| c.starts_with(name)).collect();
    Some(match matching[..] {
        [":quit"] | [":exit"] => Command::Quit,
        [":help"] => Command::Help,
        [":reset"] => Command::Reset(arg.trim().to_owned()),
        [":type"] => Command::TypeOf(arg.trim().to_owned()),
        _ => Command::Unknown(name.to_owned()),
    })
}

/// What dotty prints for unknown command `name`.
pub fn unknown(name: &str) -> String {
    format!("Unknown command: \"{name}\", run \":help\" for a list of commands")
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    #[test]
    fn commands_by_prefix() {
        assert_eq!(parse(":q"), Some(Command::Quit));
        assert_eq!(parse(":exit"), Some(Command::Quit));
        assert_eq!(parse(" :help "), Some(Command::Help));
        assert_eq!(parse(":reset -deprecation"), Some(Command::Reset("-deprecation".into())));
        assert_eq!(parse(":t  List(1) "), Some(Command::TypeOf("List(1)".into())));
        assert_eq!(parse(":load f"), Some(Command::Unknown(":load".into())));
        assert_eq!(parse(":: x"), None);
        assert_eq!(parse("1 :: Nil"), None);
    }
}
