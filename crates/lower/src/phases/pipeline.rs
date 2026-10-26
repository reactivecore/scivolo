//! The order the phases run in over a unit.

use super::walk::{class_defs, retain_outside};
use super::*;
use crate::{Lower, Result};
use typer::tpd::TNodeId;

pub fn run(l: &mut Lower, root: TNodeId) -> Result<()> {
    let unit_owner = l.tpd.sym(root).or_else(|| l.u.package("")).expect("the unit's package");
    static_members::transform(l, root)?;
    refined_trees::transform(l, root);
    retained_bodies::transform(l, root)?;
    drop_for_map::transform(l, root)?;
    drop_inlined::transform(l, root)?;
    context_results::transform(l, root)?;
    trait_init::find_inits(l, &class_defs(l.tpd, root));
    module_purity::record(l, root);
    beta_reduce::transform(l, root, unit_owner)?;
    let mut classes = class_defs(l.tpd, root);
    let inits = module_purity::objects_with_inits(l, &classes);
    super_accessors::transform(l, &classes)?;
    for &td in &classes {
        synthetic::add_synthetic_members(l, root, td)?;
        enumerations::transform(l, td)?;
        package_objects::transform(l, td);
        first_transform::transform(l, td, &inits);
    }
    java_enums::transform(l, root, &classes)?;
    if expand_sams::transform(l, root)? {
        classes = class_defs(l.tpd, root);
    }
    protected_accessors::transform(l, root, &classes)?;
    let moved = extension_methods::transform(l, &classes)?;
    let deferred = mixin::methods_with_body(l, &classes, false);
    for &td in &classes {
        uninitialized_defs::transform(l, td);
        let retained = initializers::transform(l, td)?;
        getters::transform(l, td, &retained)?;
    }
    uninitialized_defs::refuse_others(l, root)?;
    for &td in &classes {
        setter_calls::transform(l, td)?;
    }
    for &td in &classes {
        interpolators::transform(l, td)?;
        by_name::transform(l, td);
        hoist_super_args::transform(l, root, td)?;
    }
    patmat::transform(l, root, unit_owner)?;
    let placeholders = inline_patterns::transform(l, root, unit_owner)?;
    classes.retain(|td| !placeholders.contains(td));
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
    elim_opaque::transform(l, root)?;
    for &td in &classes {
        explicit_self::transform(l, td)?;
    }
    explicit_outer::transform(l, &classes)?;
    drop_imports::transform(l, root);
    let erased = erasure::erased_defs::drop(l, root, &inits)?;
    retain_outside(l.tpd, &mut classes, erased);
    let pending = local_classes::transform(l, root, &classes)?;
    lambda_lift::transform(l, &classes, pending)?;
    for &td in &classes {
        value_classes::transform(l, td)?;
        specialize::transform(l, td)?;
        specialize_tuples::transform(l, td, &inits)?;
        repeated::transform(l, td)?;
        resolve_super::transform(l, td)?;
        function_xxl_forwarders::transform(l, td)?;
        tuple_optimizations::transform(l, td);
        let_over_apply::transform(l, td);
        erasure::transform(l, td)?;
        bridges::transform(l, td, &deferred)?;
    }
    type_tests::renumber_tested_values(l);
    drop_inlined::unmark_calls(l);
    array_apply::transform(l, &classes)?;
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
