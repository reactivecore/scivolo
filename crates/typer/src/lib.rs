//! A typer for Scala 3 scripts: a port of dotty's typing algorithms for the scripting subset,
//! over `syntax` trees, with symbols and types in `tasty::Universe` (docs/research/09-typer.md).
#![allow(non_upper_case_globals)]

mod ctx;
mod defn;
pub mod dump;
mod main_proxies;
mod namer;
mod symbols;
pub mod tpd;
mod typer;

use crate::ctx::{Ctx, Level, Mode};
use syntax::ast::{Ast, NodeId, Span};
use syntax::diag::Diagnostics;
use syntax::names::Names;
use tasty::Universe;
use tpd::{TNodeId, Tpd};

pub use typer::Typer;

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

/// Types the desugared unit `root` of `ast`, parsed with language features `features`, defining
/// its symbols in `u`.
pub fn type_unit(
    u: &mut Universe,
    ast: &mut Ast,
    names: &mut Names,
    root: NodeId,
    features: syntax::config::Features,
) -> Result<Typed, Fallback> {
    let mut typer =
        Typer::new(u, ast, names, features).map_err(|what| Fallback::Unsupported { at: Span::NONE, what })?;
    let ctx = root_context(&mut typer);
    let typed = typer.typed(root, typer::Pt::Any, ctx);
    if let Some((at, what)) = typer.unsupported.take() {
        return Err(Fallback::Unsupported { at, what });
    }
    if typer.diags.has_errors() {
        return Err(Fallback::Errors(typer.diags));
    }
    Ok(Typed { tpd: typer.tpd, root: typed, diags: typer.diags })
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
