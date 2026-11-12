// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/config/Feature.scala
//! Whether a language feature is enabled where typing is (dotty's `Feature.enabled`): by the
//! options, or by an import of it from `scala.language` in scope.

use super::Typer;
use crate::SourceUnit;
use crate::ctx::{Ctx, Level};
use names::nme;
use syntax::ast::{Ast, NodeId, Span, Tree};
use syntax::config::{Features, SourceVersion};
use syntax::parse::language_prefix;
use tasty::NullMode;

/// Refuses the files of `units` whose source version is unsupported, each with its root's span.
pub fn source_versions(ast: &Ast, units: &[&SourceUnit]) -> Vec<(Span, String)> {
    let refusal = |unit: &SourceUnit| {
        let what = super::check_source_version(unit.cfg.source_version).err()?;
        Some((ast.span(unit.root), what))
    };
    units.iter().filter_map(|unit| refusal(unit)).collect()
}

impl Typer<'_> {
    /// Whether feature `f`, `scala.language.name` (`name` dotted or not), is enabled in `ctx`.
    pub fn feature_enabled(&mut self, f: Features, name: &str, ctx: Ctx) -> bool {
        if self.features.contains(f) {
            return true;
        }
        let (prefix, name) = name.rsplit_once('.').unwrap_or(("", name));
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l)
                && let Some(enabled) = self.mentions_feature(i, prefix, name)
            {
                return enabled;
            }
            level = self.levels.outer(l);
        }
        false
    }

    /// Whether import `i` enables (`Some(true)`) or excludes (`Some(false)`) feature `name` of
    /// `scala.language.prefix`; `None` where it does not name it or is no language import, which
    /// the names of its path decide (dotty's `mentionsFeature`).
    fn mentions_feature(&mut self, i: usize, prefix: &str, name: &str) -> Option<bool> {
        if self.is_root_import(i) {
            return None;
        }
        let Some(tree) = self.imports[i].tree() else { return self.typed_mentions_feature(i, prefix, name) };
        let Tree::Import { expr, selectors } = *self.ast.tree(tree) else { return None };
        if language_prefix(self.ast, self.names, expr)? != prefix {
            return None;
        }
        let selectors = self.ast.items(selectors).to_vec();
        let (_, rename) =
            selectors.iter().find_map(|&s| self.selector_names(s).filter(|(n, _)| n == name))?;
        Some(rename != nme::WILDCARD)
    }

    /// `mentions_feature` for import `i` of a typed path, which its path's symbol decides.
    fn typed_mentions_feature(&mut self, i: usize, prefix: &str, name: &str) -> Option<bool> {
        let object = match prefix {
            "" => "scala.language".to_owned(),
            _ => format!("scala.language.{prefix}"),
        };
        let info = self.import_info(i)?;
        let (qual, enabled) =
            (info.qual, info.selectors.iter().find(|s| s.name == name)?.rename != nme::WILDCARD);
        let site = self.import_site(qual)?;
        (self.u.full_name(site).replace('$', "") == object).then_some(enabled)
    }

    /// Takes the features and source version of source file `file` of a unit of several (dotty's
    /// `CompilationUnit` settings); the file it took them from before.
    pub fn enter_file(&mut self, file: usize) -> usize {
        let outer = self.file;
        let Some(cfg) = self.file_cfgs.get(file).copied() else { return outer };
        if file != outer {
            let v = cfg.source_version;
            self.features = cfg.features;
            self.source_future = v.requires_new_syntax();
            self.dollar_names_warned = v.is_at_least(SourceVersion::V3_9);
            self.scala2 = v.is_scala2();
            self.file = file;
        }
        outer
    }

    /// Runs `f` with the settings of the source file of `tree`, which is typed apart from its file.
    pub fn in_file_of<R>(&mut self, tree: NodeId, f: impl FnOnce(&mut Self) -> R) -> R {
        let span = self.ast.span(tree);
        let outer = if span.exists() { self.enter_file(self.file_of(span)) } else { self.file };
        let r = f(self);
        self.enter_file(outer);
        r
    }

    /// How comparisons in `ctx` treat `Null`: explicit nulls, and safe without `unsafeNulls`.
    pub fn null_mode(&mut self, ctx: Ctx) -> NullMode {
        let explicit = self.explicit_nulls;
        let safe = explicit && !self.feature_enabled(Features::UNSAFE_NULLS, nme::UNSAFE_NULLS, ctx);
        NullMode { explicit, safe }
    }
}
