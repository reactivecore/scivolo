//! The lowering and code generation of a typed unit of several files, and of the files of a
//! refused unit that the typer kept, to find the refusals of the stages after typing in the same
//! round as the typer's.

use crate::units::{Attributed, at_point, file_at, refused};
use crate::{Backend, Lower};
use std::panic::{AssertUnwindSafe, catch_unwind};
use syntax::config::Config;
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::source::{SourceFile, Sources};
use tasty::Universe;
use typer::Typed;
use typer::pickle::Pickled;
use typer::tpd::TNodeId;

/// The output of `generate` for typed unit `typed` of `files`, each with its settings, and its
/// warnings; or the refusal of the lowering or the backend, attributed to the file it lies in.
pub fn lowered<R>(
    u: &mut Universe,
    mut typed: Typed,
    (names, fresh): (&mut Names, &mut FreshNames),
    (files, cfgs): (&[SourceFile], &[Config]),
    (backend, generate): (Backend, impl FnOnce(&mut Lower, TNodeId, Vec<Pickled>) -> crate::Result<R>),
) -> Result<(R, Diagnostics), Attributed> {
    let whole = |e| Attributed { refusal: refused(e), files: Vec::new() };
    let sources = Sources::new(files).map_err(whole)?;
    let located = Sources::new(files).map_err(whole)?;
    let pickles = std::mem::take(&mut typed.pickles);
    let mut in_backend = false;
    let generate = |l: &mut Lower, root| generate(l, root, pickles).inspect_err(|_| in_backend = true);
    let trees = (&mut typed.tpd, &mut typed.diags);
    let lowered =
        crate::lower_trees(u, trees, typed.root, (names, fresh), (sources, cfgs), (backend, generate));
    let out = lowered.map_err(|e| {
        let stage = match in_backend && backend == Backend::Jvm {
            true => "the JVM backend",
            false => "the lowering",
        };
        let what = format!("{stage} does not support {}{}", e.what, at_point(&located, e.at));
        Attributed { refusal: refused(what), files: file_at(&located, e.at) }
    })?;
    Ok((out, typed.diags))
}

/// The refusal of the lowering or the backend of `kept`, the files of a refused unit no refusal
/// or type error lies in, if it lies in a file (dotty's `Run.compileUnits` reporting the errors
/// of all units); none if they lower or a stage panics on the trees of the files they need.
pub fn lowered_kept<R>(
    u: &mut Universe,
    kept: Typed,
    names: (&mut Names, &mut FreshNames),
    unit: (&[SourceFile], &[Config]),
    backend: (Backend, impl FnOnce(&mut Lower, TNodeId, Vec<Pickled>) -> crate::Result<R>),
) -> Option<Attributed> {
    let lowered = catch_unwind(AssertUnwindSafe(|| lowered(u, kept, names, unit, backend)));
    match lowered {
        Ok(Err(refusal)) if !refusal.files.is_empty() => Some(refusal),
        _ => None,
    }
}
