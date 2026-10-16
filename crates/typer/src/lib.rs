//! A typer for Scala 3 scripts: a port of dotty's typing algorithms for the scripting subset,
//! over `syntax` trees, with symbols and types in `tasty::Universe` (docs/research/09-typer.md).
#![allow(non_upper_case_globals)]

mod completion;
mod constraint;
mod ctx;
mod defn;
pub mod dump;
pub mod inlining;
mod main_proxies;
mod namer;
mod signatures;
mod symbols;
pub mod tpd;
mod typer;
pub mod unpickle;

use crate::ctx::{Ctx, Level, Mode};
use syntax::ast::{Ast, NodeId, Span, Tree};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use tasty::Universe;
use tpd::{TNodeId, Tpd};

pub use completion::Request;
pub use typer::{RETAINED_BODY, Typer, fold_binop, fold_unop};

/// Why a unit takes the JVM frontend instead.
pub enum Fallback {
    /// A construct outside the scripting subset.
    Unsupported { at: Span, what: String },
    /// Type errors, with the diagnostics.
    Errors(Diagnostics),
}

/// A typed unit: its trees and warnings.
pub struct Typed {
    pub tpd: Tpd,
    pub root: TNodeId,
    pub diags: Diagnostics,
}

/// Types the desugared unit `root` of `ast`, parsed with options `cfg`, defining its symbols in
/// `u`, and expands its inline calls; `fresh` holds the counters of the unit's fresh names.
pub fn type_unit(
    u: &mut Universe,
    ast: &mut Ast,
    (names, fresh): (&mut Names, &mut FreshNames),
    root: NodeId,
    cfg: syntax::config::Config,
) -> Result<Typed, Fallback> {
    Units::default().type_unit(u, ast, (names, fresh), root, cfg)
}

/// The typer's knowledge of the script symbols of the units typed so far into one universe.
#[derive(Default)]
pub struct Units {
    syms: symbols::Syms,
    errors_first: bool,
    /// Whether typing stops before the inline calls expand (dotty's `-Ystop-after:typer`).
    typer_only: bool,
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
        root: NodeId,
        cfg: syntax::config::Config,
    ) -> Result<Typed, Fallback> {
        let mut typer = Typer::new(u, (ast, root), (names, fresh), cfg, &mut self.syms)
            .map_err(|what| Fallback::Unsupported { at: Span::NONE, what })?;
        let ctx = root_context(&mut typer);
        let typed = typer.typed(root, typer::Pt::Any, ctx);
        self.finish(typer, typed)
    }

    /// Types the units of one `ast`, each the root of a source file with the offset it starts at,
    /// in increasing order, as dotty's namer and typer do: enters the symbols of all, then types
    /// each. The result's root is a package clause of the root package holding the typed units.
    pub fn type_units(
        &mut self,
        u: &mut Universe,
        ast: &mut Ast,
        (names, fresh): (&mut Names, &mut FreshNames),
        units: &[(syntax::source::Offset, NodeId)],
        cfg: syntax::config::Config,
    ) -> Result<Typed, Fallback> {
        let roots: Vec<NodeId> = units
            .iter()
            .flat_map(|&(_, r)| match *ast.tree(r) {
                Tree::Thicket { trees } => ast.items(trees).to_vec(),
                Tree::Empty => Vec::new(),
                _ => vec![r],
            })
            .collect();
        let trees = ast.list(&roots);
        let unit = ast.node(Tree::Thicket { trees });
        let mut typer = Typer::new(u, (ast, unit), (names, fresh), cfg, &mut self.syms)
            .map_err(|what| Fallback::Unsupported { at: Span::NONE, what })?;
        typer.file_starts = units.iter().skip(1).map(|&(start, _)| start).collect();
        let ctx = root_context(&mut typer);
        typer.index(&roots, ctx);
        typer.check_split_companions();
        let typed: Vec<TNodeId> = roots.iter().map(|&r| typer.typed(r, typer::Pt::Any, ctx)).collect();
        let root = typer.u.root();
        let tpe = typer.u.intern_type(tasty::Type::Package(root));
        let name = syntax::ast::TName { name: typer.names.intern("_root_"), is_type: false };
        let pid = typer.tpd.node(tpd::Tree::Ident { name }, Span::NONE, Some(tpe), Some(root));
        let stats = typer.tpd.list(&typed);
        let unit = typer.tpd.node(tpd::Tree::PackageDef { pid, stats }, Span::NONE, Some(tpe), None);
        self.finish(typer, unit)
    }

    /// Completes typed unit `typed`: its annotations, type variables and inline calls.
    fn finish(&mut self, mut typer: Typer, typed: TNodeId) -> Result<Typed, Fallback> {
        typer.typed_annotations();
        typer.check_dynamic_calls();
        if let Err(what) = typer.zonk_trees() {
            typer.unsupported_at(Span::NONE, what);
        }
        if !self.typer_only
            && typer.unsupported.is_none()
            && !typer.diags.has_errors()
            && let Err((at, what)) = typer.with_inlining(false, |i| i.run(typed))
        {
            typer.unsupported_at(at, what);
        }
        self.syms = std::mem::take(&mut typer.syms);
        self.syms.end_unit(typer.u);
        if self.errors_first && typer.diags.has_errors() {
            return Err(Fallback::Errors(typer.diags));
        }
        if let Some((at, what)) = typer.unsupported.take() {
            return Err(Fallback::Unsupported { at, what });
        }
        if typer.diags.has_errors() {
            return Err(Fallback::Errors(typer.diags));
        }
        Ok(Typed { tpd: typer.tpd, root: typed, diags: typer.diags })
    }

    /// Removes the object `name` from the root package, and it and the definitions in it from
    /// the script symbols: no later unit refers to them or is told of their names.
    pub fn forget(&mut self, u: &mut Universe, name: &str) {
        let root = u.root();
        let objects: Vec<tasty::SymbolId> = [name.to_owned(), format!("{name}$")]
            .iter()
            .flat_map(|n| u.decl(root, n).unwrap_or_default())
            .collect();
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

/// The context of a unit: the members of the root package, then the wildcard imports of
/// `java.lang`, `scala` and `Predef` (dotty's `rootImports`), outermost first.
fn root_context(t: &mut Typer) -> Ctx {
    let root = t.u.root();
    let java_lang = t.u.intern_type(tasty::Type::Package(t.defn.java_lang));
    let scala = t.u.intern_type(tasty::Type::Package(t.defn.scala));
    let predef = defn::type_ref(t.u, t.defn.predef);
    let level = t.levels.push(None, Level::Members(root));
    let level = t.root_import(Some(level), java_lang);
    let level = t.root_import(Some(level), scala);
    let level = t.root_import(Some(level), predef);
    Ctx { owner: root, level, mode: Mode::NONE }
}
