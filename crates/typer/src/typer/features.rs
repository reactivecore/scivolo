// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/config/Feature.scala
//! Whether a language feature is enabled where typing is (dotty's `Feature.enabled`): by the
//! options, or by an import of it from `scala.language` in scope.

use super::Typer;
use crate::SourceUnit;
use crate::ctx::{Ctx, Level};
use names::nme;
use syntax::ast::{Ast, NodeId, Span};
use syntax::config::{Features, SourceVersion};
use tasty::NullMode;

/// Refuses the files of `units` whose source version is unsupported or, where the first is
/// `2.13`, is not, or the other way round; each with its root's span.
pub fn source_versions(ast: &Ast, units: &[&SourceUnit]) -> Vec<(Span, String)> {
    let scala2 = units.first().is_some_and(|unit| unit.cfg.source_version.is_scala2());
    let refusal = |unit: &SourceUnit| {
        let v = unit.cfg.source_version;
        let mixed =
            (v.is_scala2() != scala2).then(|| "source version `2.13` in some files of a unit only".into());
        let what = super::check_source_version(v).err().or(mixed)?;
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
                let (qual, enabled) = (info.qual, s.rename != nme::WILDCARD);
                let site = self.import_site(qual);
                if site.is_some_and(|s| self.u.full_name(s).replace('$', "") == object) {
                    return enabled;
                }
            }
            level = self.levels.outer(l);
        }
        false
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
            self.tpd.scala2 = self.scala2;
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
