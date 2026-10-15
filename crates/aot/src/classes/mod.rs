//! The classes, modules and traits the program uses: hierarchy, method lookup and vtable slots.

mod layout;

pub use layout::array_names;

use crate::R;
use nir::{DefnKind, Global, GlobalId, StrId};
use rustc_hash::{FxHashMap, FxHashSet};
use vm::World;

pub type ClassId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Class,
    Module,
    Trait,
}

pub struct Field {
    pub offset: u32,
    pub ty: nir::TypeId,
}

pub struct Class {
    pub name: GlobalId,
    pub kind: Kind,
    pub parent: Option<ClassId>,
    pub traits: Vec<ClassId>,
    pub ancestors: FxHashSet<ClassId>,
    pub fields: FxHashMap<GlobalId, Field>,
    pub size: u32,
    /// The virtual methods in vtable order: the parent's, then the ones this class declares first.
    pub slots: Vec<StrId>,
    pub slot_of: FxHashMap<StrId, u32>,
}

#[derive(Default)]
pub struct Classes {
    pub all: Vec<Class>,
    by_name: FxHashMap<GlobalId, ClassId>,
}

impl Classes {
    pub fn get(&self, id: ClassId) -> &Class {
        &self.all[id as usize]
    }

    /// The class named `name`, linking it and its ancestors on first use.
    pub fn link(&mut self, world: &mut World, name: GlobalId) -> R<ClassId> {
        if let Some(&id) = self.by_name.get(&name) {
            return Ok(id);
        }
        let defn = world.defn(name).map_err(|e| e.to_string())?;
        let (kind, parent, traits) = match &defn.kind {
            DefnKind::Class { parent, traits } => (Kind::Class, *parent, traits.clone()),
            DefnKind::Module { parent, traits } => (Kind::Module, *parent, traits.clone()),
            DefnKind::Trait { traits } => (Kind::Trait, None, traits.clone()),
            _ => return Err(format!("{} is not a class", world.name(name))),
        };
        let parent = parent.map(|p| self.link(world, p)).transpose()?;
        let traits = traits.into_iter().map(|t| self.link(world, t)).collect::<R<Vec<_>>>()?;
        let id = self.all.len() as ClassId;
        let mut ancestors = FxHashSet::from_iter([id]);
        for &a in parent.iter().chain(&traits) {
            ancestors.extend(self.get(a).ancestors.iter().copied());
        }
        let (fields, size) = self.fields(world, name, parent)?;
        let (slots, slot_of) = match kind {
            Kind::Trait => Default::default(),
            _ => self.slots(world, name, parent)?,
        };
        self.all.push(Class { name, kind, parent, traits, ancestors, fields, size, slots, slot_of });
        self.by_name.insert(name, id);
        Ok(id)
    }

    pub fn link_top(&mut self, world: &mut World, name: &str) -> R<ClassId> {
        let g = world.top(name);
        self.link(world, g)
    }

    pub fn find(&self, name: GlobalId) -> Option<ClassId> {
        self.by_name.get(&name).copied()
    }

    /// Where methods of `cls` are looked up: its class chain, then the traits of each class in
    /// the chain, later-mixed traits first.
    fn lookup_order(&self, cls: ClassId) -> Vec<ClassId> {
        let mut order = Vec::new();
        let mut c = Some(cls);
        while let Some(id) = c {
            order.push(id);
            c = self.get(id).parent;
        }
        for id in order.clone() {
            self.push_traits(id, &mut order);
        }
        order
    }

    fn push_traits(&self, id: ClassId, order: &mut Vec<ClassId>) {
        for &t in self.get(id).traits.iter().rev() {
            if !order.contains(&t) {
                order.push(t);
                self.push_traits(t, order);
            }
        }
    }

    /// The first concrete definition of `sig` in `cls`'s lookup order.
    pub fn resolve(&self, world: &mut World, cls: ClassId, sig: StrId) -> Option<GlobalId> {
        self.lookup_order(cls).into_iter().find_map(|id| {
            let g = GlobalId(world.pool.globals.intern(Global::Member(self.get(id).name, sig)));
            world.defn(g).ok().filter(|d| matches!(d.kind, DefnKind::Define { .. })).map(|_| g)
        })
    }
}

/// Whether `sig` names a public instance method, the methods vtables and itables hold.
pub fn is_virtual(sig: &str) -> bool {
    sig.starts_with('D') && sig.ends_with("EO")
}

/// The itable key of method `sig`: its 64-bit FNV-1a hash.
pub fn method_key(sig: &str) -> u64 {
    sig.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}
