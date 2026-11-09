// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The warning about `$` in a definition's name (dotty's `checkDefName`).

use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::{Attrs, NodeId};
use tasty::SymbolId;

impl Typer<'_> {
    /// Warns if `name` of definition `tree` with flags `flags` owned by `owner` contains a `$`
    /// (before the trailing one of a module class).
    pub fn check_def_name(&mut self, tree: NodeId, name: &str, flags: FlagSet, owner: SymbolId) {
        let span = self.ast.span(tree);
        let synthetic_owner = || self.syms.flags.get(&owner).is_some_and(|o| o.is(f::SYNTHETIC));
        let module = flags.is(f::MODULE) && name.ends_with('$');
        let dollars = name.len() == 1 || name.len() == 2 && name.ends_with('$');
        let exempt = !self.dollar_names_warned
            || self.ast.attrs(tree).contains(Attrs::BACKQUOTED)
            || span.is_synthetic()
            || !span.exists()
            || flags.is_one_of(f::SYNTHETIC.or(f::ACCESSOR).or(f::CASE_ACCESSOR))
            || flags.is(f::PARAM) && synthetic_owner()
            || flags.is(f::JAVA_DEFINED)
            || dollars
            || !module && (self.fresh.made(name) || is_default_getter(name))
            || self.ast.attrs(tree).contains(Attrs::EVIDENCE) && name.starts_with("evidence$");
        let max = if module { name.len() - 1 } else { name.len() };
        if exempt || !name[..max].contains('$') {
            return;
        }
        let shown = if module { &name[..max] } else { name };
        let start = span.point();
        let msg = format!(
            "The identifier `{shown}` should not contain `$`, which is reserved for internal compiler use."
        );
        self.diags.warning_at(start, start + max as u32, start, msg);
    }
}

/// Whether `name` is `<method>$default$<n>` (dotty's `DefaultGetterName`).
fn is_default_getter(name: &str) -> bool {
    name.rsplit_once("$default$").is_some_and(|(m, n)| !m.is_empty() && n.parse::<u32>().is_ok())
}
