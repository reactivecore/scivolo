//! Runtime type information: every class's `java.lang.Class` object, laid out as Scala Native expects.

use crate::class::{self, ClassId, Kind};
use crate::gcinfo::FIRST_ARRAY;
use crate::heap;
use crate::{Vm, VmError};

/// The primitive classes in id order, with the name `Class.getName` reports for each.
pub const PRIMITIVES: [(&str, &str); 11] = [
    ("PrimitiveByte", "byte"),
    ("PrimitiveShort", "short"),
    ("PrimitiveInt", "int"),
    ("PrimitiveLong", "long"),
    ("PrimitiveChar", "char"),
    ("PrimitiveFloat", "float"),
    ("PrimitiveDouble", "double"),
    ("PrimitiveBoolean", "boolean"),
    ("PrimitiveUnit", "void"),
    ("RawSize", "size"),
    ("RawPtr", "pointer"),
];

/// The `java.lang.Class` fields the collector reads, at their offsets in its C `Rtti` struct.
pub const GC_RTTI_FIELDS: [(&str, usize); 5] =
    [("F2idO", 8), ("F4nameO", 24), ("F4sizeO", 32), ("F15refFieldOffsetsO", 40), ("F10superClassO", 64)];

/// The interfaces `Class.getInterfaces` reports for a class with `parent` and direct `traits`:
/// the traits neither the parent nor another direct trait extends; for an array class, the
/// traits of `runtime.Array` (`array_traits`) in reverse. `extends(a, b)`: whether `a` is or
/// extends `b`.
pub fn interfaces(
    parent: Option<u32>,
    traits: &[u32],
    array_traits: Option<&[u32]>,
    extends: impl Fn(u32, u32) -> bool,
) -> Vec<u32> {
    if let Some(array_traits) = array_traits {
        return array_traits.iter().rev().copied().collect();
    }
    let implied =
        |t: u32| parent.is_some_and(|p| extends(p, t)) || traits.iter().any(|&u| u != t && extends(u, t));
    traits.iter().copied().filter(|&t| !implied(t)).collect()
}

/// The name `Class.getName` reports for class `name`.
pub fn java_name(name: &str) -> &str {
    let short = name.strip_prefix("scala.scalanative.runtime.").unwrap_or("");
    match PRIMITIVES.iter().find(|(p, _)| *p == short) {
        Some((_, java)) => java,
        None if short == "BoxedUnit$" => "scala.runtime.BoxedUnit",
        None => name,
    }
}

#[derive(Default)]
pub struct Rtti {
    meta: Option<ClassId>,
    meta_size: usize,
    pending: Vec<ClassId>,
    unnamed: Vec<ClassId>,
    strings_ready: bool,
    next_class: i32,
    next_trait: i32,
}

impl Vm {
    pub fn bootstrap(&mut self) -> Result<(), VmError> {
        self.rtti.next_class = 23;
        self.update_gc_ids();
        let meta = self.link_top("java.lang.Class")?;
        self.rtti.meta = Some(meta);
        self.rtti.meta_size = self.class(meta).size as usize;
        for (sig, offset) in GC_RTTI_FIELDS {
            if self.meta_field(sig)? != offset {
                return Err(VmError::Unsupported(format!(
                    "java.lang.Class::{sig} is not where Immix reads it"
                )));
            }
        }
        for id in std::mem::take(&mut self.rtti.pending) {
            self.init_rtti(id)?;
        }
        self.link_rtti(meta)?;
        self.link_top("java.lang.String")?;
        self.link_top("scala.scalanative.runtime.CharArray")?;
        self.rtti.strings_ready = true;
        for id in std::mem::take(&mut self.rtti.unnamed) {
            self.name_rtti(id)?;
        }
        Ok(())
    }

    pub fn init_rtti(&mut self, id: ClassId) -> Result<(), VmError> {
        let Some(meta) = self.rtti.meta.or_else(|| (self.class_name(id) == "java.lang.Class").then_some(id))
        else {
            self.rtti.pending.push(id);
            return Ok(());
        };
        let meta_size = self.class(meta).size as usize;
        let addr = self.heap.permanent(meta_size + 8, heap::Blob::Object);
        self.classes.all[id as usize].rtti = addr;
        heap::write(addr, self.class(meta).rtti);
        heap::write(addr + meta_size, id);
        let class_id = match self.weak_class_id(id)? {
            Some(weak) => weak,
            None => self.class_id_value(id),
        };
        let size = self.class(id).size as i32;
        heap::write(addr + self.meta_field("F2idO")?, class_id);
        heap::write(addr + self.meta_field("F4sizeO")?, size);
        heap::write(addr + self.meta_field("F12idRangeUntilO")?, class_id);
        let offsets = self.ref_field_offsets(id);
        heap::write(addr + self.meta_field("F15refFieldOffsetsO")?, offsets);
        self.note_gc_class(id, class_id);
        self.link_rtti(id)?;
        match self.rtti.strings_ready {
            true => self.name_rtti(id),
            false => {
                self.rtti.unnamed.push(id);
                Ok(())
            }
        }
    }

    /// Points the metadata of `id` at the metadata of its superclass and its interfaces: its
    /// direct traits in declaration order without those its superclass or another direct trait
    /// extends; for an array class, `runtime.Array`'s traits `Cloneable` and `Serializable`.
    fn link_rtti(&mut self, id: ClassId) -> Result<(), VmError> {
        let array_class = self.world.top("scala.scalanative.runtime.Array");
        let c = self.class(id);
        let parent = c.parent.map_or(0, |p| self.class(p).rtti);
        let array = c.parent.filter(|&p| self.class(p).name == array_class);
        let array_traits = array.map(|p| &self.class(p).traits[..]);
        let traits: Vec<usize> = interfaces(c.parent, &c.traits, array_traits, |a, b| self.is_subclass(a, b))
            .into_iter()
            .map(|t| self.class(t).rtti)
            .collect();
        let rtti = c.rtti;
        let interfaces = self.heap.permanent(8 * traits.len(), heap::Blob::Words);
        for (i, t) in traits.iter().enumerate() {
            heap::write(interfaces + 8 * i, *t);
        }
        heap::write(rtti + self.meta_field("F10superClassO")?, parent);
        heap::write(rtti + self.meta_field("F15interfacesCountO")?, traits.len() as i32);
        heap::write(rtti + self.meta_field("F10interfacesO")?, interfaces);
        Ok(())
    }

    fn meta_field(&mut self, sig: &str) -> Result<usize, VmError> {
        let g = self.world.top("java.lang.Class");
        let g = self.world.member(g, sig);
        self.field_offset(g)
    }

    fn name_rtti(&mut self, id: ClassId) -> Result<(), VmError> {
        let name = java_name(self.class_name(id)).to_owned();
        let s = self.new_string_in(&name, crate::object::Space::Permanent)?;
        let at = self.class(id).rtti + self.meta_field("F4nameO")?;
        heap::write(at, s);
        Ok(())
    }

    fn class_id_value(&mut self, id: ClassId) -> i32 {
        let name = self.class_name(id).to_owned();
        let short = name.strip_prefix("scala.scalanative.runtime.").unwrap_or("");
        if let Some(i) = PRIMITIVES.iter().position(|(p, _)| *p == short) {
            return i as i32;
        }
        if let Some(i) = class::array_names().position(|a| a == short) {
            return FIRST_ARRAY + i as i32;
        }
        match (name.as_str(), self.class(id).kind) {
            ("java.lang.Object", _) => 11,
            ("scala.scalanative.runtime.Array", _) => 12,
            (_, Kind::Trait) => {
                self.rtti.next_trait -= 1;
                self.rtti.next_trait
            }
            _ => {
                self.rtti.next_class += 1;
                self.rtti.next_class
            }
        }
    }

    pub fn class_name(&self, id: ClassId) -> &str {
        self.world.pool.top_name(self.class(id).name)
    }

    /// The class of the object at `obj`, read from its header.
    pub fn class_of(&self, obj: usize) -> ClassId {
        self.class_of_rtti(heap::read(obj))
    }

    /// The class whose `java.lang.Class` object is at `rtti`.
    pub fn class_of_rtti(&self, rtti: usize) -> ClassId {
        heap::read(rtti + self.rtti.meta_size)
    }
}
