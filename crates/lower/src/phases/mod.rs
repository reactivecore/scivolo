//! dotty's phases between the typer and NIR generation, each a pass over the unit.

mod bridges;
mod build;
mod constructors;
mod erasure;
mod first_transform;
mod getters;
mod memoize;
mod methods;
mod mixin;
mod synthetic;
mod walk;

use crate::{Lower, Result};
use typer::tpd::{TNodeId, Tpd, Tree};

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let classes = class_defs(l.tpd, root);
    for &td in &classes {
        synthetic::add_synthetic_members(l, td)?;
    }
    for &td in &classes {
        getters::transform(l, td)?;
    }
    for &td in &classes {
        erasure::transform(l, td)?;
        bridges::transform(l, td)?;
    }
    let unit_classes: Vec<_> = classes.iter().filter_map(|&td| l.tpd.sym(td)).collect();
    let concrete = mixin::concrete_trait_methods(l, &classes);
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
