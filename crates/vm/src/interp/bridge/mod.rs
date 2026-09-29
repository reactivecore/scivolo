//! Bridge methods (ADR 0008): for a method `m(A…): R` a class lacks, the one method `m` of the
//! same arity it can call by boxing, unboxing or casting, called virtually.

mod body;

use crate::Vm;
use crate::class::ClassId;
use crate::interp::Flow;
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
        let Some((name, wanted)) = nir::demangle::method(self.world.pool.str(sig)) else { return Ok(None) };
        let mut found: Vec<(u32, GlobalId, Vec<String>)> = Vec::new();
        for id in self.lookup_order(cls) {
            let owner = self.class(id).name;
            for m in self.world.members(owner)?.to_vec() {
                let Some(msig) = self.world.pool.member_sig(m).map(str::to_owned) else { continue };
                let Some((n, tys)) = nir::demangle::method(&msig) else { continue };
                let last = tys.len().saturating_sub(1);
                let costs = tys.iter().zip(&wanted).enumerate().map(|(i, (t, w))| match i == last {
                    true => cost(t, w),
                    false => cost(w, t),
                });
                let total: Option<u32> = costs.sum();
                let overridden = found.iter().any(|(_, g, _)| self.world.pool.member_sig(*g) == Some(&msig));
                if n == name
                    && tys.len() == wanted.len()
                    && !overridden
                    && let Some(total) = total
                    && matches!(self.world.defn(m)?.kind, DefnKind::Define { .. })
                {
                    found.push((total, m, tys));
                }
            }
        }
        let Some(best) = found.iter().map(|(c, ..)| *c).min() else { return Ok(None) };
        let best: Vec<_> = found.into_iter().filter(|(c, ..)| *c == best).collect();
        let [(_, target, tys)] = &best[..] else { return Ok(None) };
        let name = GlobalId(self.world.pool.globals.intern(Global::Member(self.class(cls).name, sig)));
        let defn = self.bridge_defn(name, &wanted, *target, tys)?;
        self.world.define(defn);
        Ok(Some(name))
    }
}
