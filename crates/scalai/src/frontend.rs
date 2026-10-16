//! The own frontend for scripts: parser, typer and lowering into NIR files.

use libimage::Image;
use libs::SCALA;
use lower::directives::{directive_options, without_header};
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
    // Temporary until #118 (script dependencies): `//> using dep` makes a script take scala-cli.
    options.extend(directive_options(text, SCALA)?);
    let cfg = Config::checked(&options).map_err(|o| format!("option `{o}`"))?;
    let image = image.ok_or("no library image")?;
    let name = source.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let source = SourceFile::new(name, without_header(text));
    let lowered = catch_unwind(AssertUnwindSafe(|| {
        lower::compile_with(&mut Universe::open_image(image), &source, cfg)
    }));
    let (files, warnings) =
        lowered.map_err(|p| lower::output::panicked(p.as_ref()))?.map_err(|r| Refused {
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
    Ok(lower::output::write_nir(files, classes)?)
}
