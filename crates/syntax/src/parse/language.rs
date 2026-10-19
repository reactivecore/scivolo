// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//! Language imports: features and the source version they enable.

use super::Parser;
use crate::ast::{NodeId, Tree};
use crate::config::{Features, SourceVersion};

const GLOBAL_FEATURES: [&str; 4] = ["pureFunctions", "captureChecking", "separationChecking", "safe"];

impl Parser<'_> {
    /// `Some(prefix)` if `path` is `[[_root_.]scala.]language[.prefix]` (dotty's `languageImport`).
    pub fn language_prefix(&self, path: NodeId) -> Option<String> {
        let text = |n: crate::ast::TName| self.s.names.text(n.name);
        let named = |t: Option<NodeId>, want: &str| -> Option<Option<NodeId>> {
            match *self.ast.tree(t?) {
                Tree::Ident { name } if text(name) == want && !name.is_type => Some(None),
                Tree::Select { qualifier, name } if text(name) == want && !name.is_type => {
                    Some(Some(qualifier))
                }
                _ => None,
            }
        };
        let is_empty = |t: Option<NodeId>| t.is_none_or(|t| matches!(self.ast.tree(t), Tree::Empty));
        if let Tree::Select { qualifier, name } = *self.ast.tree(path)
            && matches!(text(name), "experimental" | "deprecated")
        {
            return (self.language_prefix(qualifier).as_deref() == Some("")).then(|| text(name).to_string());
        }
        let q = named(Some(path), "language")?;
        if is_empty(q) {
            return Some(String::new());
        }
        let q2 = named(q, "scala")?;
        let root_ok = is_empty(q2) || named(q2, "_root_").is_some_and(|q3| q3.is_none());
        root_ok.then(String::new)
    }

    /// Enable the features `sels` import from `language.prefix`, or set the source version.
    pub fn language_import(&mut self, prefix: &str, sels: &[NodeId], outermost: bool) {
        for &sel in sels {
            let Tree::ImportSelector { imported, renamed: None, .. } = *self.ast.tree(sel) else { continue };
            let Tree::Ident { name } = *self.ast.tree(imported) else { continue };
            let name = self.s.names.text(name.name).to_string();
            let span = self.ast.span(imported);
            let qualified = if prefix.is_empty() { name.clone() } else { format!("{prefix}.{name}") };
            if prefix == "experimental" && GLOBAL_FEATURES.contains(&name.as_str()) && !outermost {
                self.syntax_error_span("this language import is only allowed at the toplevel", span);
            }
            if let Some(f) = Features::named(&qualified) {
                self.s.cfg.features.insert(f);
            }
            if !prefix.is_empty() || !is_source_version_name(&name) {
                continue;
            }
            if !outermost {
                self.syntax_error_span("source version import is only allowed at the toplevel", span);
            } else if self.unit_version_set {
                self.syntax_error_span("duplicate source version import", span);
            } else if matches!(name.as_str(), "3.1-migration" | "never") {
                let stem = name.replace("-migration", "");
                let hint = if is_source_version_name(&stem) {
                    format!(", did you mean language.`{stem}`?")
                } else {
                    String::new()
                };
                self.syntax_error_span(format!("`{name}` is not a valid source version{hint}"), span);
            } else {
                self.unit_version_set = true;
                if let Some(v) = SourceVersion::parse(&name) {
                    self.s.cfg.source_version = v;
                }
            }
        }
    }
}

fn is_source_version_name(name: &str) -> bool {
    matches!(name, "2.13" | "never") || SourceVersion::parse(name).is_some()
}
