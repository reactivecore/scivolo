//! Whether a language feature is enabled where typing is (dotty's `Feature.enabled`): by the
//! options, or by an import of it from `scala.language` in scope.

use super::Typer;
use crate::ctx::{Ctx, Level};
use syntax::config::Features;

impl Typer<'_> {
    /// Whether feature `f`, `scala.language.name` (`name` dotted or not), is enabled in `ctx`.
    pub fn feature_enabled(&mut self, f: Features, name: &str, ctx: Ctx) -> bool {
        if self.features.contains(f) {
            return true;
        }
        let (object, name) = match name.rsplit_once('.') {
            Some((prefix, last)) => (format!("scala.language.{prefix}"), last),
            None => ("scala.language".to_owned(), name),
        };
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l)
                && !self.is_root_import(i)
                && let Some(info) = self.import_info(i)
                && let Some(s) = info.selectors.iter().find(|s| s.name == name)
            {
                let (qual, enabled) = (info.qual, s.rename != "_");
                let site = self.import_site(qual);
                if site.is_some_and(|s| self.u.full_name(s).replace('$', "") == object) {
                    return enabled;
                }
            }
            level = self.levels.outer(l);
        }
        false
    }
}
