//! dotty's phases between the typer and NIR generation, each a pass over the unit.

mod accessor_names;
pub(crate) mod adapt;
mod adapted;
mod array_apply;
mod array_literals;
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
mod enum_values;
mod enumerations;
mod envelope;
mod erased_branches;
mod erasure;
mod eta_reduce;
mod expand_private;
mod explicit_outer;
mod first_transform;
mod getters;
mod initializers;
mod interpolators;
mod lambda_lift;
mod lazy_init;
mod lazy_trees;
pub(crate) mod lazy_vals;
mod library_traits;
mod lifted;
mod literals;
mod local_classes;
mod local_free;
mod local_lazy;
mod local_lift;
mod memoize;
mod methods;
mod mirrors;
mod mixin;
mod outer_args;
mod patmat;
mod repeated;
mod returns;
pub(crate) mod select_static;
mod sensical;
mod specialize;
mod synthetic;
mod tail_calls;
mod tailrec;
mod trait_fields;
mod trait_init;
mod type_tests;
mod value_classes;
pub(crate) mod walk;

use crate::{Lower, Result};
use typer::tpd::{TNodeId, Tpd, Tree};

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let classes = class_defs(l.tpd, root);
    for &td in &classes {
        synthetic::add_synthetic_members(l, root, td)?;
        enumerations::transform(l, td)?;
        first_transform::transform(l, td);
    }
    let deferred = mixin::methods_with_body(l, &classes, false);
    trait_fields::find_inits(l, &classes);
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
        getters::record_rhs(l, td);
    }
    for &td in &classes {
        eta_reduce::transform(l, td);
    }
    local_lazy::transform(l, root)?;
    explicit_outer::transform(l, &classes)?;
    local_classes::transform(l, root, &classes)?;
    lambda_lift::transform(l, &classes)?;
    for &td in &classes {
        value_classes::transform(l, td)?;
        specialize::transform(l, td)?;
        repeated::transform(l, td)?;
        erasure::transform(l, td)?;
        bridges::transform(l, td, &deferred)?;
    }
    array_apply::transform(l, &classes);
    for &td in &classes {
        select_static::transform(l, td);
        tailrec::transform(l, td);
    }
    let (traits, others): (Vec<TNodeId>, Vec<TNodeId>) =
        classes.iter().partition(|&&td| l.tpd.sym(td).is_some_and(|c| crate::symbols::is_trait(l.u, c)));
    for &td in traits.iter().chain(&others) {
        mixin::transform(l, td)?;
    }
    for &td in &classes {
        lazy_vals::transform(l, td)?;
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
    expand_private::transform(l, &classes);
    Ok(())
}

/// The class definitions of unit `root`, local ones too, each before the classes it contains.
fn class_defs(tpd: &Tpd, root: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        match *tpd.tree(t) {
            Tree::TypeDef { rhs, .. } if !matches!(tpd.tree(rhs), Tree::Template { .. }) => continue,
            Tree::TypeDef { .. } => out.push(t),
            _ => {}
        }
        todo.extend(walk::children(tpd, t).into_iter().rev());
    }
    out
}
