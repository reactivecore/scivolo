//! The header of a script source: a `#!` line and the `//> using` directives before the code.

/// The keys of the `//> using` directive for compiler options.
const OPTIONS: [&str; 6] =
    ["option", "options", "scalacOption", "scalacOptions", "scalac-option", "scalac-options"];

/// The lines before the code of `text`: blank lines, comments and a first line starting with `#!`.
fn header(text: &str) -> impl Iterator<Item = &str> {
    let mut in_block = false;
    text.lines().enumerate().map_while(move |(i, line)| {
        let l = line.trim();
        if in_block {
            in_block = !l.contains("*/");
            return Some(line);
        }
        if let Some(rest) = l.strip_prefix("/*") {
            in_block = !rest.contains("*/");
            return Some(line);
        }
        (l.is_empty() || l.starts_with("//") || (i == 0 && l.starts_with("#!"))).then_some(line)
    })
}

/// What the `//> using` directives of a source say.
#[derive(Debug, PartialEq)]
pub struct Directives<'t> {
    pub options: Vec<&'t str>,
    /// The dependencies, each with the key naming it.
    pub deps: Vec<(&'t str, &'t str)>,
}

/// The `//> using` directives of `text`, or the first directive the own frontend does not
/// honour: any but options, dependencies and `scala` with version `scala` or 3.9.
pub fn directives<'t>(text: &'t str, scala: &str) -> Result<Directives<'t>, String> {
    let (mut options, mut deps) = (Vec::new(), Vec::new());
    for line in header(text) {
        let Some(rest) = line.trim_start().strip_prefix("//> using") else { continue };
        let mut words = rest.split_whitespace().map(|w| w.trim_matches('"'));
        let key = words.next().unwrap_or("");
        let values: Vec<&str> = words.collect();
        match key {
            "scala" if matches!(values[..], [v] if v == "3.9" || v == scala) => {}
            k if OPTIONS.contains(&k) => options.extend(values),
            "dep" | "deps" | "dependency" | "dependencies" => {
                deps.extend(values.into_iter().map(|v| (key, v)))
            }
            _ => return Err(format!("`//> using {key}`")),
        }
    }
    Ok(Directives { options, deps })
}

/// `text` with a first line starting with `#!` blanked, offsets kept.
pub fn without_header(text: &str) -> String {
    match text.strip_prefix("#!") {
        Some(_) => {
            let end = text.find(['\r', '\n']).unwrap_or(text.len());
            " ".repeat(end) + &text[end..]
        }
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Directives, directives, without_header};

    const SCALA: &str = "3.9.0";

    fn deps(text: &str) -> Vec<&str> {
        directives(text, SCALA).unwrap().deps.into_iter().map(|(_, d)| d).collect()
    }

    #[test]
    fn directives_are_read_before_the_code_only() {
        let text = "#!/usr/bin/env scivolo\n\n// c\n//> using dep x\nobject A\n//> using dep y\n";
        assert_eq!(deps(text), ["x"]);
        let text = "/*\n  License text\n*/\n/* c */\n//> using dep z\n@main def m() = ()\n";
        assert_eq!(deps(text), ["z"]);
        assert!(deps("object A\n//> using dep y\n").is_empty());
    }

    #[test]
    fn accepts_the_scala_version_options_and_dependency_directives() {
        let none = Directives { options: vec![], deps: vec![] };
        let ok = "//> using scala 3.9.0\n//> using scala 3.9\n// using dep x\n";
        assert_eq!(directives(ok, SCALA), Ok(none));
        let options = "//> using options -Werror \"-Wnonunit-statement\"\n//> using option -nowarn\n";
        assert_eq!(
            directives(options, SCALA).unwrap().options,
            ["-Werror", "-Wnonunit-statement", "-nowarn"]
        );
        assert_eq!(directives("//> using scala 3.8.3", SCALA), Err("`//> using scala`".into()));
        let dep = "  //> using dep com.lihaoyi::os-lib:0.11.4";
        let one = vec![("dep", "com.lihaoyi::os-lib:0.11.4")];
        assert_eq!(directives(dep, SCALA), Ok(Directives { options: vec![], deps: one }));
        assert_eq!(deps("//> using deps a::b:1 \"c::d:2\""), ["a::b:1", "c::d:2"]);
        assert_eq!(directives("//> using test.dep a::b:1", SCALA), Err("`//> using test.dep`".into()));
    }

    #[test]
    fn blanks_a_shebang_line() {
        assert_eq!(
            without_header("#!/usr/bin/env scivolo\nobject A"),
            format!("{}\nobject A", " ".repeat(22))
        );
        assert_eq!(without_header("object A"), "object A");
    }
}
