//! What the collector learns from class metadata: reference field offsets, and the class ids of
//! arrays, boxed pointers and weak references; and the program's weak reference handler.

use crate::class::{self, ClassId};
use crate::heap::{self, Blob, GcIds};
use crate::interp::Flow;
use crate::{Vm, VmError};
use nir::Type;

/// The id of the first array class; `scala.scalanative.runtime.Array` has the one before.
pub const FIRST_ARRAY: i32 = 13;

/// Weak reference classes take ids from here on, so they form the one range the collector
/// checks; other class ids stay far below.
const FIRST_WEAK: i32 = 1 << 24;

pub struct GcClasses {
    boxed_ptr: i32,
    weak: Option<ClassId>,
    next_weak: i32,
    weak_referent: i32,
    /// The program's function to run after a collection cleared weak references.
    weak_handler: Option<u64>,
}

impl Default for GcClasses {
    fn default() -> Self {
        Self { boxed_ptr: -1, weak: None, next_weak: FIRST_WEAK, weak_referent: -1, weak_handler: None }
    }
}

impl Vm {
    /// `scalanative_GC_set_weak_references_collected_callback(f)`: `f` runs at the next call
    /// after such a collection, not inside the allocation that collected.
    pub fn set_weak_handler(&mut self, f: u64) {
        self.gc_classes.weak_handler = (f != 0).then_some(f);
        self.heap.watch_weak_refs();
    }

    pub fn run_weak_handler(&mut self) -> Flow<()> {
        if let Some(f) = self.gc_classes.weak_handler {
            self.call_value(f, &[])?;
        }
        Ok(())
    }

    /// The class id of `id` if it is `java.lang.ref.WeakReference` or a subclass.
    pub fn weak_class_id(&mut self, id: ClassId) -> Result<Option<i32>, VmError> {
        if self.class_name(id) == "java.lang.ref.WeakReference" {
            let referent = self.world.member(self.class(id).name, "F21_gc_modified_referentO");
            self.gc_classes.weak = Some(id);
            self.gc_classes.weak_referent = self.field_offset(referent)? as i32;
        }
        match self.gc_classes.weak {
            Some(weak) if self.is_subclass(id, weak) => {
                let class_id = self.gc_classes.next_weak;
                self.gc_classes.next_weak += 1;
                self.update_gc_ids();
                Ok(Some(class_id))
            }
            _ => Ok(None),
        }
    }

    /// Records the class id of `id` if the collector treats its instances specially.
    pub fn note_gc_class(&mut self, id: ClassId, class_id: i32) {
        if self.class_name(id) == "scala.scalanative.unsafe.Ptr" {
            self.gc_classes.boxed_ptr = class_id;
            self.update_gc_ids();
        }
    }

    /// The offsets of `id`'s reference fields as the collector reads them: `i32`s ending in -1.
    pub fn ref_field_offsets(&mut self, id: ClassId) -> usize {
        let pool = &self.world.pool;
        let mut offsets: Vec<i32> = self
            .class(id)
            .fields
            .values()
            .filter(|f| {
                matches!(pool.ty(f.ty), Type::Ref { .. } | Type::Array { .. } | Type::Null | Type::Unit)
            })
            .map(|f| f.offset as i32)
            .collect();
        offsets.sort_unstable();
        offsets.push(-1);
        let addr = self.heap.permanent(4 * offsets.len(), Blob::Data);
        for (i, o) in offsets.iter().enumerate() {
            heap::write(addr + 4 * i, *o);
        }
        addr
    }

    pub fn update_gc_ids(&self) {
        let position = |name| class::array_names().position(|a| a == name).unwrap_or(0) as i32;
        let gc = &self.gc_classes;
        self.heap.set_ids(&GcIds {
            object_array: FIRST_ARRAY + position("ObjectArray"),
            blob_array: FIRST_ARRAY + position("BlobArray"),
            arrays_min: FIRST_ARRAY - 1,
            arrays_max: FIRST_ARRAY + class::array_names().count() as i32 - 1,
            boxed_ptr: gc.boxed_ptr,
            weak_refs_min: FIRST_WEAK,
            weak_refs_max: gc.next_weak - 1,
            weak_referent_offset: gc.weak_referent,
        });
    }
}
