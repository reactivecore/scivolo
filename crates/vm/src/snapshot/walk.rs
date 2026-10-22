//! The typed walk of what the VM's tables reach in the trained region: every reference must
//! stay inside it, and every other pointer is kept, named as a `Target`, or refuses the
//! snapshot. Unreached training garbage is not walked.

use super::{DENIED, Target, Walk};
use crate::heap::{self, Blob, Trained};
use crate::interp::{Func, FuncId, func_of_ref, is_func_ref};
use crate::layout::{self, ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, natives};
use nir::{Type, TypeId};
use std::ops::Range;

/// Pointer-typed words below this are integers, such as file descriptors and sizes.
const SMALL: u64 = 1 << 16;

struct Walker<'a> {
    vm: &'a Vm,
    region: Range<usize>,
    blobs: &'a [(usize, Blob)],
    seen: Vec<bool>,
    queue: Vec<usize>,
    out: Walk,
}

/// Walks what `roots`, addresses the VM's tables hold, reach in what training filled.
pub fn walk(vm: &Vm, Trained { range: region, blobs }: Trained, roots: &[usize]) -> Result<Walk, String> {
    let out = Walk { bytes: region.len(), ..Walk::default() };
    let mut w = Walker { vm, region, blobs, seen: vec![false; blobs.len()], queue: Vec::new(), out };
    for &root in roots {
        if !w.region.contains(&root) {
            return Err(format!("the VM holds {root:#x}, outside the trained region"));
        }
        w.reach(root);
    }
    while let Some(i) = w.queue.pop() {
        let (addr, blob) = blobs[i];
        let end = blobs.get(i + 1).map_or(w.region.end, |&(next, _)| next);
        match blob {
            Blob::Object => w.object(addr),
            Blob::Words => w.words(addr, end),
            Blob::Typed(ty) => w.value(addr, ty),
            Blob::Data => Ok(()),
        }
        .map_err(|e| format!("{e}, in {}", w.describe(addr, blob)))?;
    }
    Ok(w.out)
}

impl Walker<'_> {
    /// Queues the allocation holding `addr`, inside the region, unless it was reached before.
    fn reach(&mut self, addr: usize) {
        let i = self.blobs.partition_point(|&(start, _)| start <= addr) - 1;
        if !std::mem::replace(&mut self.seen[i], true) {
            self.queue.push(i);
        }
    }

    fn object(&mut self, obj: usize) -> Result<(), String> {
        let vm = self.vm;
        let Some(cls) = self.class_of(obj) else { return Err(format!("object at {obj:#x} has no class")) };
        let c = vm.class(cls);
        if let Some(&a) = c.ancestors.iter().find(|&&a| DENIED.contains(&vm.class_name(a))) {
            return Err(format!("an instance of {} depends on the environment", vm.class_name(a)));
        }
        self.out.objects += 1;
        self.reach(heap::read(obj));
        if c.array_elem.is_none() {
            return c.fields.values().try_for_each(|f| self.value(obj + f.offset as usize, f.ty));
        }
        let len = heap::read::<i32>(obj + ARRAY_LENGTH) as usize;
        let data = obj + ARRAY_HEADER as usize;
        match vm.class_name(cls).strip_prefix("scala.scalanative.runtime.") {
            Some("ObjectArray") => (0..len).try_for_each(|i| self.reference(data + 8 * i)),
            Some("BlobArray") => self.words(data, data + len),
            _ => Ok(()),
        }
    }

    /// The class of `obj` if its header points to class metadata in the region.
    fn class_of(&self, obj: usize) -> Option<u32> {
        let rtti = heap::read::<usize>(obj);
        let cls = self.region.contains(&rtti).then(|| self.vm.class_of_rtti(rtti))?;
        self.vm.classes.all.get(cls as usize).is_some_and(|c| c.rtti == rtti).then_some(cls)
    }

    fn describe(&self, addr: usize, blob: Blob) -> String {
        match (blob, self.class_of(addr)) {
            (Blob::Object, Some(cls)) => format!("an instance of {}", self.vm.class_name(cls)),
            _ => format!("the {blob:?} at {addr:#x}"),
        }
    }

    /// The value of type `ty` at `at`.
    fn value(&mut self, at: usize, ty: TypeId) -> Result<(), String> {
        let pool = &self.vm.world.pool;
        match pool.ty(ty) {
            Type::Ref { .. } | Type::Array { .. } | Type::Null | Type::Unit => self.reference(at),
            Type::Ptr => self.pointer(at),
            Type::ArrayValue(..) | Type::StructValue(_) => {
                self.words(at, at + layout::size_of(pool, ty) as usize)
            }
            _ => Ok(()),
        }
    }

    fn reference(&mut self, at: usize) -> Result<(), String> {
        match heap::read::<usize>(at) {
            0 => Ok(()),
            v if self.region.contains(&v) => {
                self.reach(v);
                Ok(())
            }
            v => Err(format!("reference {v:#x} at {at:#x} leaves the region")),
        }
    }

    /// Every aligned word in `from..to`, each possibly a pointer.
    fn words(&mut self, from: usize, to: usize) -> Result<(), String> {
        (from.next_multiple_of(8)..to.saturating_sub(7)).step_by(8).try_for_each(|at| self.pointer(at))
    }

    fn pointer(&mut self, at: usize) -> Result<(), String> {
        let v = heap::read::<u64>(at);
        match classify(v, &self.region, |v| target(self.vm, v)) {
            Ok(Some(t)) => self.out.targets.push((at - self.region.start, t)),
            Ok(None) if self.region.contains(&(v as usize)) => self.reach(v as usize),
            Ok(None) => {}
            Err(()) => return Err(format!("pointer {v:#x} at {at:#x} has no name outside this process")),
        }
        Ok(())
    }
}

/// How word `v` of a pointer type is restored: as it is, rewritten to what `name` names, or not
/// at all.
fn classify(
    v: u64,
    region: &Range<usize>,
    name: impl FnOnce(u64) -> Option<Target>,
) -> Result<Option<Target>, ()> {
    if v < SMALL || region.contains(&(v as usize)) {
        return Ok(None);
    }
    name(v).map(Some).ok_or(())
}

fn target(vm: &Vm, v: u64) -> Option<Target> {
    if is_func_ref(v) {
        let (top, sig) = func_name(vm, func_of_ref(v))?;
        return Some(Target::Function(top, sig));
    }
    if let Some(f) = vm.callbacks.target(v as usize) {
        let (top, sig) = func_name(vm, f)?;
        return Some(Target::Trampoline(top, sig));
    }
    natives::symbol_at(v as usize).map(Target::Symbol)
}

fn func_name(vm: &Vm, f: FuncId) -> Option<(String, String)> {
    let (Func::Nir { name, .. } | Func::Native { name, .. }) = vm.funcs.all.get(f as usize)? else {
        return None;
    };
    let pool = &vm.world.pool;
    Some((pool.top_name(*name).to_owned(), pool.member_sig(*name).unwrap_or("").to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers_outside_the_region_need_a_name() {
        let region = 0x2000_0000_0000..0x2000_0001_0000;
        let rust_heap = Box::new(0u64);
        let addr = &*rust_heap as *const u64 as u64;
        let symbol = || Some(Target::Symbol("environ".into()));
        assert_eq!(classify(0, &region, |_| None), Ok(None));
        assert_eq!(classify(3, &region, |_| None), Ok(None));
        assert_eq!(classify(0x2000_0000_0040, &region, |_| None), Ok(None));
        assert_eq!(classify(addr, &region, |_| None), Err(()));
        assert_eq!(classify(addr, &region, |_| symbol()), Ok(symbol()));
    }
}
