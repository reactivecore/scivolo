// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   repl/src/dotty/tools/repl/ParseResult.scala
//! The REPL's commands: `:` and a letter, then an argument (dotty's `ParseResult.commands`).

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Reset(String),
    Imports,
    Load(String),
    TypeOf(String),
    Silent,
    /// A command of dotty's this REPL does not run.
    Unsupported(&'static str),
    Ambiguous(String, Vec<&'static str>),
    Unknown(String),
}

pub const COMMANDS: [&str; 15] = [
    ":quit",
    ":exit",
    ":help",
    ":reset",
    ":imports",
    ":jar",
    ":kind",
    ":load",
    ":require",
    ":dep",
    ":type",
    ":doc",
    ":settings",
    ":sh",
    ":silent",
];

pub const HELP: &str = "The REPL has several commands available:

:help                    print this summary
:load <path>             interpret lines in a file
:quit                    exit the interpreter
:type <expression>       evaluate the type of the given expression
:imports                 show import history
:reset                   reset the repl to its initial state, forgetting all session entries
:silent                  disable/enable automatic printing of results";

/// What dotty prints for the commands it no longer runs.
const RETIRED: [(&str, &str); 3] = [
    (":kind", "The :kind command is not currently supported."),
    (":require", ":require is no longer supported, but has been replaced with :jar. Please use :jar"),
    (":sh", "The :sh command is deprecated. Use `import scala.sys.process._` and `\"command\".!` instead."),
];

/// Java's `\s`.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r')
}

/// The command `text` is, if it is `:`, a letter, more non-space characters, and an argument on
/// the same line (dotty's `(:[a-zA-Z]\S*)\s*(.*)`); a prefix names the one command it starts.
pub fn parse(text: &str) -> Option<Command> {
    let rest = text.strip_prefix(':')?;
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let (name, arg) = text.split_at(text.find(is_space).unwrap_or(text.len()));
    let arg = arg.trim_start_matches(is_space);
    if arg.contains(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']) {
        return None;
    }
    let matching: Vec<&'static str> = COMMANDS.into_iter().filter(|c| c.starts_with(name)).collect();
    Some(match matching[..] {
        [] => Command::Unknown(name.to_owned()),
        [":quit"] | [":exit"] => Command::Quit,
        [":help"] => Command::Help,
        [":reset"] => Command::Reset(arg.trim().to_owned()),
        [":imports"] => Command::Imports,
        [":load"] => Command::Load(arg.to_owned()),
        [":type"] => Command::TypeOf(arg.trim().to_owned()),
        [":silent"] => Command::Silent,
        [other] => Command::Unsupported(other),
        _ => Command::Ambiguous(name.to_owned(), matching),
    })
}

/// What dotty prints for unknown command `name`.
pub fn unknown(name: &str) -> String {
    format!("Unknown command: \"{name}\", run \":help\" for a list of commands")
}

/// What dotty prints for command prefix `name` that starts all of `matching`.
pub fn ambiguous(name: &str, matching: &[&str]) -> String {
    let matching = matching.join(", ");
    format!(
        "\"{name}\" matches {matching}. Try typing a few more characters. Run \":help\" for a list of commands"
    )
}

/// What dotty prints for command `name`, if it no longer runs it.
pub fn retired(name: &str) -> Option<&'static str> {
    RETIRED.iter().find(|(n, _)| *n == name).map(|(_, text)| *text)
}

#[cfg(test)]
mod tests {
    use super::{Command, ambiguous, parse};

    #[test]
    fn commands_by_prefix() {
        assert_eq!(parse(":q"), Some(Command::Quit));
        assert_eq!(parse(":exit"), Some(Command::Quit));
        assert_eq!(parse(":help "), Some(Command::Help));
        assert_eq!(parse(" :help"), None);
        assert_eq!(parse("\n:type 1\n"), None);
        assert_eq!(parse(":reset -deprecation"), Some(Command::Reset("-deprecation".into())));
        assert_eq!(parse(":t  List(1) "), Some(Command::TypeOf("List(1)".into())));
        assert_eq!(parse(":si"), Some(Command::Silent));
        assert_eq!(parse(":i"), Some(Command::Imports));
        assert_eq!(parse(":l a b "), Some(Command::Load("a b ".into())));
        assert_eq!(parse(":load\n f"), Some(Command::Load("f".into())));
        assert_eq!(parse(":settings -explain"), Some(Command::Unsupported(":settings")));
        assert_eq!(parse(":frob"), Some(Command::Unknown(":frob".into())));
        assert_eq!(parse(":type 1 +\n2"), None);
        assert_eq!(parse(":: x"), None);
        assert_eq!(parse("1 :: Nil"), None);
    }

    #[test]
    fn ambiguous_prefixes() {
        let Some(Command::Ambiguous(name, matching)) = parse(":s") else { panic!() };
        assert_eq!(matching, [":settings", ":sh", ":silent"]);
        let text = "\":s\" matches :settings, :sh, :silent. Try typing a few more characters. Run \":help\" for a list of commands";
        assert_eq!(ambiguous(&name, &matching), text);
    }
}
