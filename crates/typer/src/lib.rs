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
use syntax::ast::{Ast, NodeId, Span};
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
        typer.typed_annotations();
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
