//! dotty's phases between the typer and NIR generation, each a pass over the unit.

pub(crate) mod adapt;
mod bridges;
mod build;
pub(crate) mod casts;
mod constructors;
mod erasure;
mod first_transform;
mod getters;
mod memoize;
mod methods;
mod mixin;
mod repeated;
mod synthetic;
pub(crate) mod walk;

use crate::{Lower, Result};
use typer::tpd::{TNodeId, Tpd, Tree};

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let classes = class_defs(l.tpd, root);
    for &td in &classes {
        synthetic::add_synthetic_members(l, td)?;
        first_transform::transform(l, td);
    }
    let deferred = mixin::methods_with_body(l, &classes, false);
    for &td in &classes {
        getters::transform(l, td)?;
    }
    for &td in &classes {
        getters::setter_calls(l, td)?;
    }
    for &td in &classes {
        repeated::transform(l, td)?;
        erasure::transform(l, td)?;
        bridges::transform(l, td, &deferred)?;
    }
    let unit_classes: Vec<_> = classes.iter().filter_map(|&td| l.tpd.sym(td)).collect();
    let concrete = mixin::methods_with_body(l, &classes, true);
    for &td in &classes {
        mixin::transform(l, td, &unit_classes, &concrete)?;
    }
    for &td in &classes {
        memoize::transform(l, td)?;
    }
    for &td in &classes {
        constructors::transform(l, td)?;
    }
    Ok(())
}

/// The class definitions of unit `root`, each before the classes it contains.
fn class_defs(tpd: &Tpd, root: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        match *tpd.tree(t) {
            Tree::PackageDef { stats, .. } => todo.extend(tpd.items(stats).iter().rev()),
            Tree::TypeDef { rhs, .. } => {
                if let Tree::Template { body, .. } = *tpd.tree(rhs) {
                    out.push(t);
                    todo.extend(tpd.items(body).iter().rev());
                }
            }
            _ => {}
        }
    }
    out
}
