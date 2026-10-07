//! dotty's phases between the typer and NIR generation, each a pass over the unit.

pub(crate) mod adapt;
mod adapted;
mod bridges;
mod build;
mod by_name;
mod calls;
mod captured_vars;
mod captures;
mod case_equality;
mod case_hash;
mod case_members;
pub(crate) mod casts;
mod closures;
mod constructors;
mod envelope;
mod erased_branches;
mod erasure;
mod first_transform;
mod getters;
mod initializers;
mod interpolators;
mod lambda_lift;
mod lifted;
mod literals;
mod memoize;
mod methods;
mod mirrors;
mod mixin;
mod patmat;
mod repeated;
mod returns;
mod sensical;
mod specialize;
mod synthetic;
mod type_tests;
mod value_classes;
pub(crate) mod walk;

use crate::{Lower, Result};
use typer::tpd::{TNodeId, Tpd, Tree};

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let classes = class_defs(l.tpd, root);
    let concrete = mixin::methods_with_body(l, &classes, true);
    for &td in &classes {
        synthetic::add_synthetic_members(l, td, &concrete)?;
        first_transform::transform(l, td);
    }
    let deferred = mixin::methods_with_body(l, &classes, false);
    for &td in &classes {
        let retained = initializers::transform(l, td)?;
        getters::transform(l, td, &retained)?;
    }
    for &td in &classes {
        getters::setter_calls(l, td)?;
    }
    for &td in &classes {
        interpolators::transform(l, td)?;
        by_name::transform(l, td);
        patmat::transform(l, td)?;
        captured_vars::transform(l, td)?;
    }
    lambda_lift::transform(l, &classes)?;
    for &td in &classes {
        value_classes::transform(l, td)?;
        specialize::transform(l, td)?;
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
    for &td in &classes {
        returns::transform(l, td);
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
