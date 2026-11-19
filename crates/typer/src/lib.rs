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
mod source_units;
mod special;
mod symbols;
pub mod tpd;
mod typer;
pub mod unpickle;

use crate::ctx::Ctx;
use crate::inlining::Macros;
use syntax::ast::{Ast, NodeId, Span};
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::source::SourceFile;
use tasty::Universe;
use tpd::{TNodeId, Tpd};

pub use completion::Request;
pub use finish::SyntheticMembers;
pub use source_units::SourceUnit;
pub use typer::{RETAINED_BODY, Typer, approx, fold_binop, fold_unop, fully_simplified, match_type_rules};

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
    /// If set, where and why pickling a file's classes was refused: such a file keeps its other
    /// output, without its TASTy.
    pub unpickled: Option<Vec<(Span, String)>>,
    /// Where and why the last unit was refused, the first refusal first: at most one per file and
    /// stage, and per definition the inline calls of which do not expand.
    pub refused: Vec<(Span, String)>,
    /// The type errors of the last unit if it was refused for a construct it does not support.
    pub errors: Diagnostics,
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
        let outer = u.match_type_rules;
        let typed = self.type_one_unit(u, ast, (names, fresh), (root, cfg), macros);
        u.match_type_rules = outer;
        typed
    }

    fn type_one_unit(
        &mut self,
        u: &mut Universe,
        ast: &mut Ast,
        (names, fresh): (&mut Names, &mut FreshNames),
        (root, cfg): (NodeId, syntax::config::Config),
        macros: Option<&mut dyn Macros>,
    ) -> Result<Typed, Fallback> {
        self.refused.clear();
        self.errors = Diagnostics::default();
        let mut typer = Typer::new(u, (ast, root), (names, fresh), cfg, &mut self.syms)
            .map_err(|what| Fallback::Unsupported { at: Span::NONE, what })?;
        typer.macros = macros.map(|m| m as &mut dyn Macros);
        let ctx = Ctx::root(&mut typer);
        let typed = typer.typed(root, typer::Pt::Any, ctx);
        self.finish(typer, typed, &[typed])
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
