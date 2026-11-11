//! A typer for Scala 3 scripts: a port of dotty's typing algorithms for the scripting subset,
//! over `syntax` trees, with symbols and types in `tasty::Universe` (docs/research/09-typer.md).
#![allow(non_upper_case_globals)]

mod case_members;
mod completion;
mod constraint;
mod ctx;
mod defn;
pub mod dump;
mod finish;
pub mod inlining;
mod main_proxies;
mod namer;
pub mod pickle;
pub mod protected_access;
pub mod sealed;
mod signatures;
mod special;
mod symbols;
pub mod tpd;
mod typer;
pub mod unpickle;

use crate::ctx::Ctx;
use crate::inlining::Macros;
use syntax::ast::{Ast, NodeId, Span, Tree};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::source::SourceFile;
use tasty::Universe;
use tpd::{TNodeId, Tpd};

pub use completion::Request;
pub use finish::SyntheticMembers;
pub use typer::{RETAINED_BODY, Typer, approx, fold_binop, fold_unop};

/// Why a unit takes the JVM frontend instead.
pub enum Fallback {
    /// A construct outside the scripting subset.
    Unsupported { at: Span, what: String },
    /// Type errors, with the diagnostics.
    Errors(Diagnostics),
}

/// A typed unit: its trees, warnings and the TASTy of its top-level classes if asked for.
pub struct Typed {
    pub tpd: Tpd,
    pub root: TNodeId,
    pub diags: Diagnostics,
    pub pickles: Vec<pickle::Pickled>,
}

/// A source file of a unit of several: its desugared root, the offset it starts at and the
/// settings its parser ended with, its language imports added.
#[derive(Clone, Copy)]
pub struct SourceUnit {
    pub root: NodeId,
    pub start: syntax::source::Offset,
    pub cfg: syntax::config::Config,
}

/// Types the desugared unit `root` of `ast`, parsed with options `cfg`, defining its symbols in
/// `u`, and expands its inline calls, its macro calls through `macros`; `fresh` holds the
/// counters of the unit's fresh names.
pub fn type_unit(
    u: &mut Universe,
    ast: &mut Ast,
    (names, fresh): (&mut Names, &mut FreshNames),
    (root, cfg): (NodeId, syntax::config::Config),
    macros: Option<&mut dyn Macros>,
) -> Result<Typed, Fallback> {
    Units::default().type_unit(u, ast, (names, fresh), (root, cfg), macros)
}

/// The typer's knowledge of the script symbols of the units typed so far into one universe.
#[derive(Default)]
pub struct Units {
    syms: symbols::Syms,
    errors_first: bool,
    /// Whether typing stops before the inline calls expand (dotty's `-Ystop-after:typer`).
    typer_only: bool,
    /// The paths of the next unit's source files, relative to the source root.
    pub source_names: Vec<String>,
    /// What adds the synthetic members of a unit's classes before its inline calls expand.
    pub synthetic_members: Option<SyntheticMembers>,
    /// The next unit's source files, each at its offset among the sources, if the TASTy of its
    /// top-level classes is pickled before its inline calls expand.
    pub tasty_sources: Option<Vec<(SourceFile, syntax::source::Offset)>>,
}

impl Units {
    /// Units whose type errors take precedence over an unsupported construct of the same unit
    /// (stress mode).
    pub fn errors_first() -> Self {
        Self { errors_first: true, ..Self::default() }
    }

    /// Sets whether the next units are typed without expanding their inline calls.
    pub fn set_typer_only(&mut self, typer_only: bool) {
        self.typer_only = typer_only;
    }

    /// Types unit `root` like `type_unit`, the symbols of the units typed before counting as
    /// script symbols; its inline calls expand unless `typer_only` is set.
    pub fn type_unit(
        &mut self,
        u: &mut Universe,
        ast: &mut Ast,
        (names, fresh): (&mut Names, &mut FreshNames),
        (root, cfg): (NodeId, syntax::config::Config),
        macros: Option<&mut dyn Macros>,
    ) -> Result<Typed, Fallback> {
        let mut typer = Typer::new(u, (ast, root), (names, fresh), cfg, &mut self.syms)
            .map_err(|what| Fallback::Unsupported { at: Span::NONE, what })?;
        typer.macros = macros.map(|m| m as &mut dyn Macros);
        let ctx = Ctx::root(&mut typer);
        let typed = typer.typed(root, typer::Pt::Any, ctx);
        self.finish(typer, typed, &[typed])
    }

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
        for unit in units {
            typer::check_source_version(unit.cfg.source_version)
                .map_err(|what| Fallback::Unsupported { at: ast.span(unit.root), what })?;
        }
        let cfg = units.first().map_or_else(|| syntax::config::Config::from_options(&[]), |unit| unit.cfg);
        let mut typer =
            Typer::new(u, (ast, unit), (names, fresh), cfg, &mut self.syms).map_err(unsupported)?;
        typer.file_starts = units.iter().skip(1).map(|unit| unit.start).collect();
        typer.file_cfgs = units.iter().map(|unit| unit.cfg).collect();
        typer.macros = macros.map(|m| m as &mut dyn Macros);
        let ctx = Ctx::root(&mut typer);
        for (file, roots) in file_roots.iter().enumerate() {
            typer.enter_file(file);
            typer.index(roots, ctx);
        }
        typer.check_split_companions();
        let mut typed = Vec::new();
        for (file, roots) in file_roots.iter().enumerate() {
            typer.enter_file(file);
            typed.extend(roots.iter().map(|&r| typer.typed(r, typer::Pt::Any, ctx)));
        }
        let root = typer.u.root();
        let tpe = typer.u.intern_type(tasty::Type::Package(root));
        let name = syntax::ast::TName { name: typer.names.intern(names::nme::ROOT_PKG), is_type: false };
        let pid = typer.tpd.node(tpd::Tree::Ident { name }, Span::NONE, Some(tpe), Some(root));
        let stats = typer.tpd.list(&typed);
        let unit = typer.tpd.node(tpd::Tree::PackageDef { pid, stats }, Span::NONE, Some(tpe), None);
        self.finish(typer, unit, &typed)
    }

    /// Removes the object `name` from the root package, and it and the definitions in it from
    /// the script symbols: no later unit refers to them or is told of their names.
    pub fn forget(&mut self, u: &mut Universe, name: &str) {
        let root = u.root();
        let objects: Vec<tasty::SymbolId> =
            [name.to_owned(), format!("{name}$")].iter().flat_map(|n| u.decls_named(root, n)).collect();
        let under = |mut s: tasty::SymbolId| loop {
            if objects.contains(&s) {
                return true;
            }
            let Some(owner) = u.symbol(s).owner else { return false };
            s = owner;
        };
        self.syms.flags.retain(|&s, _| !under(s));
        objects.iter().for_each(|&o| u.unenter(o));
    }
}
