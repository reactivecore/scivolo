//! Tab completion in a session: the input typed in a wrapper object of its own, as `:type`
//! types it, neither run nor imported by later inputs.

use super::Session;
use crate::commands::COMMANDS;
use crate::completion::{self, Group};
use crate::{earlier, input, wrap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use syntax::source::SourceFile;
use typer::Request;

/// The prefix of the names of the wrapper objects of completions.
const PREFIX: &str = "rs$line$tab$";

impl Session {
    /// What dotty's REPL lists on Tab at byte offset `cursor` of `text`, in JLine's groups;
    /// nothing where the candidates are not all known. A panic while computing them is caught.
    pub fn complete(&mut self, text: &str, cursor: usize) -> Vec<Group> {
        if completion::inserts_tab(text) {
            return Vec::new();
        }
        let labels = if text.starts_with(':') {
            COMMANDS.iter().filter(|c| c.starts_with(text)).map(|&c| c.to_owned()).collect()
        } else {
            catch_unwind(AssertUnwindSafe(|| self.labels(text, cursor))).unwrap_or_default()
        };
        completion::listed(completion::word(text, cursor), labels)
    }

    /// The labels dotty completes at `cursor` of statements `text`: the members a selection
    /// offers there.
    fn labels(&mut self, text: &str, cursor: usize) -> Vec<String> {
        let object = self.fresh.next(&mut self.names, PREFIX);
        let object = self.names.text(object).to_owned();
        let source = SourceFile::maybe_incomplete(&object, text);
        let mut parsed = input::parse(source, &mut self.names, &mut self.fresh);
        let (stats, cursor) = (&parsed.stats, cursor as u32);
        let Some(selection) = completion::selection(&parsed.ast, &self.names, &parsed.source, stats, cursor)
        else {
            return Vec::new();
        };
        let (names, fresh) = (&mut self.names, &mut self.fresh);
        let prelude = earlier::prelude(&self.earlier);
        let Some(wrapped) = wrap::wrap(&mut parsed, (&object, false), self.res, &prelude, names, fresh)
        else {
            return Vec::new();
        };
        let Some(target) = completion::desugared(&parsed.ast, wrapped.root, selection.target) else {
            return Vec::new();
        };
        let request = Request { target, cursor, prefix: selection.prefix };
        let (ast, features) = ((&mut parsed.ast, wrapped.root), parsed.cfg.features);
        let found = self.units.complete(&mut self.u, ast, (names, fresh), features, &request);
        found.iter().map(|n| completion::label(n, selection.backticked)).collect()
    }
}
