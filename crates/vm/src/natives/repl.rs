//! The natives of the REPL's value printer `scalai.repl.Show` in the overlay.

use super::NativeFn;
use crate::Vm;
use crate::interp::Flow;
use nir::{DefnKind, Inst, Op, StrId};

const SHOW: &str = "scalai.repl.Show$";
const TO_STRING: &str = "D8toStringL16java.lang.StringEO";
const RUNTIME_TO_STRING: &str = "D9_toStringL13scala.ProductL16java.lang.StringEO";
const SCALA_RUN_TIME: &str = "scala.runtime.ScalaRunTime$";

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        (SHOW, "D15hasUserToStringL16java.lang.ObjectzEO") => has_user_to_string,
        (SHOW, "D14collectionNameL25scala.collection.IterableL16java.lang.StringEO") => collection_name,
        _ => return None,
    };
    Some(f)
}

/// `Show.hasUserToString(x)`: whether `x`'s `toString` is neither `Object`'s nor a body that only
/// returns `ScalaRunTime._toString(this)`, cast or not, as case classes are given.
fn has_user_to_string(vm: &mut Vm, args: &[u64]) -> Flow {
    let obj = args[1] as usize;
    if obj == 0 {
        return Ok(0);
    }
    let sig = StrId(vm.world.pool.strings.intern(TO_STRING.into()));
    let cls = vm.class_of(obj);
    let Some(g) = vm.resolve(cls, sig)? else { return Ok(0) };
    if vm.world.pool.top_name(vm.world.owner(g)) == "java.lang.Object" {
        return Ok(0);
    }
    let defn = vm.world.defn(g)?;
    let DefnKind::Define { body, .. } = &defn.kind else { return Ok(1) };
    let lets = body.insts.iter().filter_map(|i| match i {
        Inst::Let { op: Op::As { .. }, .. } => None,
        Inst::Let { op, .. } => Some(op),
        _ => None,
    });
    let ops: Vec<&Op> = lets.collect();
    let pool = &vm.world.pool;
    let synthesized = matches!(
        ops.as_slice(),
        [Op::Module(m), Op::Method { sig, .. }, Op::Call { .. }]
            if pool.top_name(*m) == SCALA_RUN_TIME && pool.str(*sig) == RUNTIME_TO_STRING
    );
    Ok(!synthesized as u64)
}

/// `Show.collectionName(x)`: `x.collectionClassName`, which is private to package `scala`.
fn collection_name(vm: &mut Vm, args: &[u64]) -> Flow {
    if args[1] == 0 {
        return vm.null_pointer();
    }
    let sig = StrId(vm.world.pool.strings.intern("D19collectionClassNameL16java.lang.StringEO".into()));
    let f = vm.virtual_method(args[1] as usize, sig)?;
    vm.call(f, &[args[1]])
}
