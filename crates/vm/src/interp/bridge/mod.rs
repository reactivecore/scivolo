//! Bridge methods (ADR 0008): for a method `m(A…): R` a class lacks, the one method `m` of the
//! same arity it can call by boxing, unboxing or casting, called virtually.

mod body;

use crate::class::ClassId;
use crate::interp::Flow;
use crate::{LoadError, Vm, World};
use nir::{DefnKind, Global, GlobalId, StrId};

/// The box class of each primitive mangled type.
const BOXES: [(&str, &str); 8] = [
    ("z", "java.lang.Boolean"),
    ("c", "java.lang.Character"),
    ("b", "java.lang.Byte"),
    ("s", "java.lang.Short"),
    ("i", "java.lang.Integer"),
    ("j", "java.lang.Long"),
    ("f", "java.lang.Float"),
    ("d", "java.lang.Double"),
];

fn is_ref(ty: &str) -> bool {
    ty.starts_with(['L', 'A'])
}

fn box_class(ty: &str) -> Option<&'static str> {
    BOXES.iter().find(|(p, _)| *p == ty).map(|(_, b)| *b)
}

/// The conversions a value of mangled type `from` needs to become a `to`: `Some(0)` as it is or
/// by a cast, `Some(1)` by boxing or unboxing, `None` if it cannot.
fn cost(from: &str, to: &str) -> Option<u32> {
    let boxes = (is_ref(to) && (from == "u" || box_class(from).is_some()))
        || (is_ref(from) && box_class(to).is_some());
    match () {
        _ if from == to || (is_ref(from) && is_ref(to)) || to == "u" => Some(0),
        _ if boxes => Some(1),
        _ => None,
    }
}

impl Vm {
    /// The bridge for method `sig` of class `cls`, defined on first use, if the methods `cls`
    /// has give one target of fewest conversions.
    pub fn bridge(&mut self, cls: ClassId, sig: StrId) -> Flow<Option<GlobalId>> {
        let order: Vec<GlobalId> = self.lookup_order(cls).into_iter().map(|id| self.class(id).name).collect();
        Ok(bridge(&mut self.world, &order, sig)?)
    }
}

/// The bridge for method `sig` of the class whose lookup order (by name, itself first) is
/// `order`, defined in `world` on first use, if its methods give one target of fewest
/// conversions.
pub fn bridge(world: &mut World, order: &[GlobalId], sig: StrId) -> Result<Option<GlobalId>, LoadError> {
    let Some((name, wanted)) = nir::demangle::method(world.pool.str(sig)) else { return Ok(None) };
    let mut found: Vec<(u32, GlobalId, Vec<String>)> = Vec::new();
    for &owner in order {
        for m in world.members(owner)?.to_vec() {
            let Some(msig) = world.pool.member_sig(m).map(str::to_owned) else { continue };
            let Some((n, tys)) = nir::demangle::method(&msig) else { continue };
            let last = tys.len().saturating_sub(1);
            let costs = tys.iter().zip(&wanted).enumerate().map(|(i, (t, w))| match i == last {
                true => cost(t, w),
                false => cost(w, t),
            });
            let total: Option<u32> = costs.sum();
            let overridden = found.iter().any(|(_, g, _)| world.pool.member_sig(*g) == Some(&msig));
            if n == name
                && tys.len() == wanted.len()
                && !overridden
                && let Some(total) = total
                && matches!(world.defn(m)?.kind, DefnKind::Define { .. })
            {
                found.push((total, m, tys));
            }
        }
    }
    let Some(best) = found.iter().map(|(c, ..)| *c).min() else { return Ok(None) };
    let best: Vec<_> = found.into_iter().filter(|(c, ..)| *c == best).collect();
    let [(_, target, tys)] = &best[..] else { return Ok(None) };
    let name = GlobalId(world.pool.globals.intern(Global::Member(order[0], sig)));
    let defn = body::bridge_defn(world, name, &wanted, *target, tys)?;
    world.define(defn);
    Ok(Some(name))
}
