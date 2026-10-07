//! The natives of the REPL's value printer `scalai.repl.Show` in the overlay.

use super::NativeFn;
use crate::Vm;
use crate::interp::Flow;
use nir::{DefnKind, Inst, Local, Op, Pool, StrId, Val, ValId};

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
    Ok(!synthesized(&vm.world.pool, &body.insts) as u64)
}

/// Whether `insts` only return `ScalaRunTime._toString(this)`, `this` cast or not.
fn synthesized(pool: &Pool, insts: &[Inst]) -> bool {
    let [Inst::Label(_, params), lets @ .., Inst::Ret(_)] = insts else { return false };
    let Some(this) = params.first() else { return false };
    let is_this = |v: ValId, this: &[Local]| matches!(pool.val(v), Val::Local(l, _) if this.contains(l));
    let (mut this, mut ops) = (vec![this.id], Vec::new());
    for i in lets {
        match i {
            Inst::Let { id, op: Op::As { obj, .. }, .. } if is_this(*obj, &this) => this.push(*id),
            Inst::Let { op, .. } => ops.push(op),
            _ => return false,
        }
    }
    matches!(
        ops.as_slice(),
        [Op::Module(m), Op::Method { sig, .. }, Op::Call { args, .. }]
            if pool.top_name(*m) == SCALA_RUN_TIME && pool.str(*sig) == RUNTIME_TO_STRING
                && args.last().is_some_and(|&a| is_this(a, &this))
    )
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
