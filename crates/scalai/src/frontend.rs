//! The own frontend for scripts: parser, typer and lowering into NIR files.

use crate::libs::SCALA;
use libimage::Image;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
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

/// Writes the NIR files of `source`, whose content is `text`, under `classes`, or why the own
/// frontend does not compile it.
pub fn compile(
    source: &Path,
    text: &[u8],
    image: Option<&'static Image>,
    classes: &Path,
) -> Result<(), String> {
    if source.extension().is_some_and(|e| e == "sc") {
        return Err(".sc scripts".into());
    }
    let text = std::str::from_utf8(text).map_err(|_| "a file not in UTF-8")?;
    if let Some(directive) = text.lines().find_map(foreign_directive) {
        return Err(format!("`//> using {directive}`"));
    }
    let image = image.ok_or("no library image")?;
    let name = source.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let source = SourceFile::new(name, without_header(text));
    let lowered =
        catch_unwind(AssertUnwindSafe(|| lower::compile(&mut Universe::open_image(image), &source, &[])));
    let files = lowered.map_err(|p| crate::panicked(p.as_ref()))??;
    std::fs::create_dir_all(classes).map_err(|e| format!("{}: {e}", classes.display()))?;
    for f in files {
        let path = classes.join(format!("{}.nir", f.name.replace('.', "/")));
        let dir = path.parent().unwrap_or(classes);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The key of the `//> using` directive on `line`, unless it is `scala` with the version
/// the own frontend compiles.
fn foreign_directive(line: &str) -> Option<&str> {
    let mut words = line.trim_start().strip_prefix("//> using")?.split_whitespace();
    let key = words.next().unwrap_or("");
    let version = words.next().unwrap_or("");
    let ours = key == "scala" && (version == "3.9" || version == SCALA) && words.next().is_none();
    (!ours).then_some(key)
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
    use super::{foreign_directive, without_header};

    #[test]
    fn accepts_only_the_scala_version_directive() {
        assert_eq!(foreign_directive("//> using scala 3.9.0"), None);
        assert_eq!(foreign_directive("//> using scala 3.9"), None);
        assert_eq!(foreign_directive("//> using scala 3.8.3"), Some("scala"));
        assert_eq!(foreign_directive("  //> using dep com.lihaoyi::os-lib:0.11.4"), Some("dep"));
        assert_eq!(foreign_directive("// using dep x"), None);
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
