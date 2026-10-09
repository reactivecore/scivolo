//! Whether a language feature is enabled where typing is (dotty's `Feature.enabled`): by the
//! options, or by an import of it from `scala.language` in scope.

use super::Typer;
use crate::ctx::{Ctx, Level};
use syntax::config::Features;

impl Typer<'_> {
    /// Whether feature `f`, `scala.language.name`, is enabled in `ctx`.
    pub fn feature_enabled(&mut self, f: Features, name: &str, ctx: Ctx) -> bool {
        if self.features.contains(f) {
            return true;
        }
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l)
                && !self.is_root_import(i)
                && let Some(info) = self.import_info(i)
                && info.selectors.iter().any(|s| s.name == name)
            {
                let qual = info.qual;
                let site = self.import_site(qual);
                if site.is_some_and(|s| self.u.full_name(s).trim_end_matches('$') == "scala.language") {
                    return true;
                }
            }
            level = self.levels.outer(l);
        }
        false
    }
}
