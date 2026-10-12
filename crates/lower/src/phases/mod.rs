//! dotty's phases between the typer and NIR generation, each a pass over the unit.

mod accessor_names;
pub(crate) mod adapt;
mod adapted;
mod array_apply;
mod array_literals;
mod beta_reduce;
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
mod dependencies;
mod drop_inlined;
mod elideable;
mod enum_values;
mod enumerations;
mod envelope;
mod erased_branches;
mod erasure;
mod eta_reduce;
mod expand_private;
mod explicit_outer;
mod extension_methods;
mod first_transform;
mod get_class;
mod getters;
mod hoist_super_args;
mod initializers;
mod interpolators;
mod lambda_lift;
mod lazy_init;
mod lazy_trees;
pub(crate) mod lazy_vals;
mod let_over_apply;
mod library_traits;
mod lift_names;
mod lifted;
mod lifted_calls;
mod literals;
mod local_classes;
mod local_free;
mod local_lazy;
mod local_lift;
mod local_outer;
mod local_proxies;
mod memoize;
mod methods;
mod mirrors;
mod mixin;
mod module_purity;
mod non_local_returns;
mod outer_args;
mod outer_fields;
mod outer_traits;
mod package_objects;
mod patmat;
mod pure_stats;
mod references_outer;
mod repeated;
mod resolve_super;
mod returns;
pub(crate) mod sam;
pub(crate) mod select_static;
mod sensical;
mod specialize;
mod specialize_tuples;
mod standard_interpolator;
mod super_accessors;
mod super_args;
mod synthetic;
mod tail_calls;
mod tailrec;
mod trait_fields;
mod trait_init;
mod trait_lazies;
mod type_tests;
mod value_classes;
mod value_members;
mod vc_elide;
pub(crate) mod walk;

use crate::{Lower, Result};
use rustc_hash::FxHashSet;
use typer::tpd::{TNodeId, Tpd, Tree};

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let unit_owner = l.tpd.sym(root).or_else(|| l.u.package("")).expect("the unit's package");
    drop_inlined::transform(l, root)?;
    trait_fields::find_inits(l, &class_defs(l.tpd, root));
    module_purity::record(l, root);
    beta_reduce::transform(l, root, unit_owner)?;
    let mut classes = class_defs(l.tpd, root);
    let inits = first_transform::objects_with_inits(l, &classes);
    super_accessors::transform(l, &classes)?;
    for &td in &classes {
        synthetic::add_synthetic_members(l, root, td)?;
        enumerations::transform(l, td)?;
        super_args::check(l, td)?;
        package_objects::transform(l, td);
        first_transform::transform(l, td, &inits);
    }
    let moved = extension_methods::transform(l, &classes)?;
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
        hoist_super_args::transform(l, root, td)?;
    }
    patmat::transform(l, root, unit_owner)?;
    extension_methods::forwarder_spans(l, &moved);
    for &td in &classes {
        captured_vars::transform(l, td)?;
        getters::record_rhs(l, td);
    }
    for &td in &classes {
        eta_reduce::transform(l, td);
        non_local_returns::transform(l, td)?;
    }
    let dropped = pure_stats::transform(l, root);
    retain_outside(l.tpd, &mut classes, dropped);
    local_lazy::transform(l, root)?;
    explicit_outer::transform(l, &classes)?;
    let erased = erasure::erased_defs::drop(l, root);
    retain_outside(l.tpd, &mut classes, erased);
    let pending = local_classes::transform(l, root, &classes)?;
    lambda_lift::transform(l, &classes, pending)?;
    for &td in &classes {
        value_classes::transform(l, td)?;
        specialize::transform(l, td)?;
        specialize_tuples::transform(l, td, &inits)?;
        repeated::transform(l, td)?;
        resolve_super::transform(l, td)?;
        let_over_apply::transform(l, td);
        erasure::transform(l, td)?;
        bridges::transform(l, td, &deferred)?;
    }
    drop_inlined::unmark_calls(l);
    array_apply::transform(l, &classes);
    for &td in &classes {
        select_static::transform(l, td);
        tailrec::transform(l, td);
    }
    let (traits, others): (Vec<TNodeId>, Vec<TNodeId>) =
        classes.iter().partition(|&&td| l.tpd.sym(td).is_some_and(|c| l.u.is_trait(c)));
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

/// Keeps the class definitions of `classes` that are not inside the `dropped` trees.
fn retain_outside(tpd: &Tpd, classes: &mut Vec<TNodeId>, dropped: Vec<TNodeId>) {
    let inside: FxHashSet<TNodeId> = dropped.into_iter().flat_map(|d| walk::subtrees(tpd, d)).collect();
    classes.retain(|td| !inside.contains(td));
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
