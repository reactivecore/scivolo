//! The own frontend for scripts: parser, typer and lowering into NIR files.

use crate::libs::SCALA;
use crate::mains;
use libimage::Image;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

/// Which frontend compiles a script, as `SCALAI_FRONTEND` chooses.
#[derive(Clone, Copy, PartialEq, Hash)]
pub enum Mode {
    /// Unset: the own frontend, else scala-cli with a notice.
    Own,
    /// `strict`: the own frontend only.
    Strict,
    /// `jvm`: scala-cli only.
    Jvm,
}

impl Mode {
    pub fn from_env() -> Result<Mode, String> {
        match std::env::var_os("SCALAI_FRONTEND").as_deref().map(|m| m.to_str()) {
            None | Some(Some("")) => Ok(Mode::Own),
            Some(Some("strict")) => Ok(Mode::Strict),
            Some(Some("jvm")) => Ok(Mode::Jvm),
            Some(m) => Err(format!("SCALAI_FRONTEND={}: expected strict or jvm", m.unwrap_or("?"))),
        }
    }
}

/// Why the own frontend does not compile a script, and its syntax or type errors as dotty
/// prints them; `fatal` if the script fails rather than taking scala-cli.
pub struct Refused {
    pub why: String,
    pub errors: Vec<String>,
    pub fatal: bool,
}

impl<T: Into<String>> From<T> for Refused {
    fn from(why: T) -> Self {
        Refused { why: why.into(), errors: Vec::new(), fatal: false }
    }
}

/// The keys of the `//> using` directive for compiler options.
const OPTIONS: [&str; 6] =
    ["option", "options", "scalacOption", "scalacOptions", "scalac-option", "scalac-options"];

/// Writes the NIR files of `source`, whose content is `text`, under `classes` with compiler
/// options `given` and returns the classes that define `main`, or why the own frontend does
/// not compile it.
pub fn compile(
    source: &Path,
    text: &[u8],
    image: Option<&'static Image>,
    classes: &Path,
    given: &[String],
) -> Result<Vec<String>, Refused> {
    if source.extension().is_some_and(|e| e == "sc") {
        return Err(".sc scripts".into());
    }
    let text = std::str::from_utf8(text).map_err(|_| "a file not in UTF-8")?;
    let mut options: Vec<&str> = given.iter().map(String::as_str).collect();
    options.extend(directive_options(text)?);
    let cfg = Config::checked(&options).map_err(|o| format!("option `{o}`"))?;
    let image = image.ok_or("no library image")?;
    let name = source.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let source = SourceFile::new(name, without_header(text));
    let lowered = catch_unwind(AssertUnwindSafe(|| {
        lower::compile_with(&mut Universe::open_image(image), &source, cfg)
    }));
    let (files, warnings) = lowered.map_err(|p| crate::panicked(p.as_ref()))?.map_err(|r| Refused {
        errors: if r.diags.items.is_empty() {
            Vec::new()
        } else {
            repl::compile_errors(&source, &r.diags, 80)
        },
        why: r.what,
        fatal: false,
    })?;
    if !cfg.warnings.nowarn && !warnings.items.is_empty() {
        let mut lines = repl::compile_errors(&source, &warnings, 80);
        if cfg.warnings.fatal {
            let count = lines.pop();
            lines.push("No warnings can be incurred under -Werror (or -Xfatal-warnings)".into());
            lines.extend(count);
            lines.push("1 error found".into());
            return Err(Refused { why: "warnings under -Werror".into(), errors: lines, fatal: true });
        }
        lines.iter().for_each(|line| eprintln!("{line}"));
    }
    let mut dirs = HashSet::new();
    let mut mains = Vec::new();
    for f in files {
        let path = classes.join(format!("{}.nir", f.name.replace('.', "/")));
        let dir = path.parent().unwrap_or(classes);
        if dirs.insert(dir.to_owned()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        mains.extend(mains::defined(&f.bytes).map_err(|e| format!("{}: undecodable NIR ({e})", f.name))?);
        std::fs::write(&path, f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(mains)
}

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

/// The compiler options of the `//> using` directives of `text`, or the first directive the own
/// frontend does not honour: any but options and `scala` with the version it compiles.
fn directive_options(text: &str) -> Result<Vec<&str>, String> {
    let mut options = Vec::new();
    for line in header(text) {
        let Some(rest) = line.trim_start().strip_prefix("//> using") else { continue };
        let mut words = rest.split_whitespace().map(|w| w.trim_matches('"'));
        let key = words.next().unwrap_or("");
        let values: Vec<&str> = words.collect();
        match key {
            "scala" if matches!(values[..], [v] if v == "3.9" || v == SCALA) => {}
            k if OPTIONS.contains(&k) => options.extend(values),
            _ => return Err(format!("`//> using {key}`")),
        }
    }
    Ok(options)
}

/// `text` with a first line starting with `#!` blanked, offsets kept.
fn without_header(text: &str) -> String {
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
    use super::{directive_options, without_header};

    #[test]
    fn directives_are_read_before_the_code_only() {
        let text = "#!/usr/bin/env scalai\n\n// c\n//> using dep x\nobject A\n//> using dep y\n";
        assert_eq!(directive_options(text), Err("`//> using dep`".into()));
        let text = "/*\n  License text\n*/\n/* c */\n//> using dep z\n@main def m() = ()\n";
        assert_eq!(directive_options(text), Err("`//> using dep`".into()));
        assert_eq!(directive_options("object A\n//> using dep y\n"), Ok(vec![]));
    }

    #[test]
    fn accepts_the_scala_version_and_options_directives() {
        let ok = "//> using scala 3.9.0\n//> using scala 3.9\n// using dep x\n";
        assert_eq!(directive_options(ok), Ok(vec![]));
        let options = "//> using options -Werror \"-Wnonunit-statement\"\n//> using option -nowarn\n";
        assert_eq!(directive_options(options), Ok(vec!["-Werror", "-Wnonunit-statement", "-nowarn"]));
        assert_eq!(directive_options("//> using scala 3.8.3"), Err("`//> using scala`".into()));
        let dep = "  //> using dep com.lihaoyi::os-lib:0.11.4";
        assert_eq!(directive_options(dep), Err("`//> using dep`".into()));
    }

    #[test]
    fn blanks_a_shebang_line() {
        assert_eq!(
            without_header("#!/usr/bin/env scalai\nobject A"),
            format!("{}\nobject A", " ".repeat(21))
        );
        assert_eq!(without_header("object A"), "object A");
    }
}
