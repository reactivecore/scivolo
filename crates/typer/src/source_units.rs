//! Typing the source files of one compilation run together.

use crate::ctx::Ctx;
use crate::inlining::Macros;
use crate::{Fallback, Typed, Typer, Units, finish, typer};
use syntax::ast::{Ast, NodeId, Span, Tree};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::Universe;

/// A source file of a unit of several: its desugared root, the offset it starts at and the
/// settings its parser ended with, its language imports added.
#[derive(Clone, Copy)]
pub struct SourceUnit {
    pub root: NodeId,
    pub start: syntax::source::Offset,
    pub cfg: syntax::config::Config,
}

impl Units {
    /// Types `units`, the source files of one `ast` in increasing order of their starts, as
    /// dotty's namer and typer do: enters the symbols of all, then types each, under its file's
    /// features and source version, expanding macro calls through `macros`. The result's root is
    /// a package clause of the root package holding the typed units.
    pub fn type_units(
        &mut self,
        u: &mut Universe,
        ast: &mut Ast,
        (names, fresh): (&mut Names, &mut FreshNames),
        units: &[SourceUnit],
        macros: Option<&mut dyn Macros>,
    ) -> Result<Typed, Fallback> {
        let outer = u.match_type_rules;
        let typed = self.type_source_units(u, ast, (names, fresh), units, macros);
        u.match_type_rules = outer;
        typed
    }

    fn type_source_units(
        &mut self,
        u: &mut Universe,
        ast: &mut Ast,
        (names, fresh): (&mut Names, &mut FreshNames),
        units: &[SourceUnit],
        macros: Option<&mut dyn Macros>,
    ) -> Result<Typed, Fallback> {
        let file_roots: Vec<Vec<NodeId>> = units
            .iter()
            .map(|unit| match *ast.tree(unit.root) {
                Tree::Thicket { trees } => ast.items(trees).to_vec(),
                Tree::Empty => Vec::new(),
                _ => vec![unit.root],
            })
            .collect();
        let roots = file_roots.concat();
        let trees = ast.list(&roots);
        let unit = ast.node(Tree::Thicket { trees });
        let unsupported = |what| Fallback::Unsupported { at: Span::NONE, what };
        let with_code: Vec<&SourceUnit> =
            units.iter().zip(&file_roots).filter(|(_, roots)| !roots.is_empty()).map(|(u, _)| u).collect();
        self.refused = typer::source_versions(ast, &with_code);
        self.errors = Diagnostics::default();
        if let Some((at, what)) = self.refused.first().cloned() {
            return Err(Fallback::Unsupported { at, what });
        }
        let first = with_code.first().copied().or(units.first());
        let cfg = first.map_or_else(|| syntax::config::Config::from_options(&[]), |unit| unit.cfg);
        let mut typer =
            Typer::new(u, (ast, unit), (names, fresh), cfg, &mut self.syms).map_err(unsupported)?;
        typer.file_starts = units.iter().skip(1).map(|unit| unit.start).collect();
        typer.file_cfgs = units.iter().map(|unit| unit.cfg).collect();
        typer.macros = macros.map(|m| m as &mut dyn Macros);
        let ctx = Ctx::root(&mut typer);
        for (file, roots) in file_roots.iter().enumerate() {
            typer.enter_file(file);
            typer.index(roots, ctx);
            self.refused.extend(typer.unsupported.take());
        }
        typer.check_split_companions();
        typer.complete_library_aliases();
        self.refused.extend(typer.unsupported.take());
        let mut typed = Vec::new();
        for (file, roots) in file_roots.iter().enumerate() {
            typer.enter_file(file);
            typed.extend(roots.iter().map(|&r| typer.typed(r, typer::Pt::Any, ctx)));
            self.refused.extend(typer.unsupported.take());
        }
        typer.typed_java_source_annotations(ctx);
        typer.check_well_known_uses();
        self.refused.extend(typer.unsupported.take());
        let unit = finish::root_package(&mut typer, &typed);
        self.finish(typer, unit, &typed)
    }
}
