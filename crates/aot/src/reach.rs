//! What the program reaches: functions to compile, classes with instances, modules, and the
//! methods called through vtables and itables.

use crate::R;
use crate::classes::{ClassId, Classes};
use nir::{DefnKind, GlobalId, StrId, Type, TypeId};
use rustc_hash::FxHashSet;
use vm::World;

pub struct Reach<'w> {
    pub world: &'w mut World,
    pub classes: Classes,
    /// Functions reached and not compiled yet.
    pub queue: Vec<GlobalId>,
    funcs: FxHashSet<GlobalId>,
    /// Classes with instances, whose vtables and itables must be filled.
    pub allocated: Vec<ClassId>,
    allocated_set: FxHashSet<ClassId>,
    /// Modules, in slot order.
    pub modules: Vec<ClassId>,
    pub vcalls: FxHashSet<StrId>,
    pub icalls: FxHashSet<StrId>,
    /// The (class, method) pairs whose implementation was reached.
    dispatched: FxHashSet<(ClassId, StrId)>,
}

impl<'w> Reach<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Reach {
            world,
            classes: Classes::default(),
            queue: Vec::new(),
            funcs: FxHashSet::default(),
            allocated: Vec::new(),
            allocated_set: FxHashSet::default(),
            modules: Vec::new(),
            vcalls: FxHashSet::default(),
            icalls: FxHashSet::default(),
            dispatched: FxHashSet::default(),
        }
    }

    pub fn link(&mut self, name: GlobalId) -> R<ClassId> {
        self.classes.link(self.world, name)
    }

    pub fn link_top(&mut self, name: &str) -> R<ClassId> {
        self.classes.link_top(self.world, name)
    }

    /// The class values of type `ty` belong to, if it is a class or array type.
    pub fn class_of_type(&mut self, ty: TypeId) -> R<Option<ClassId>> {
        Ok(match self.world.pool.ty(ty).clone() {
            Type::Ref { name, .. } => Some(self.link(name)?),
            Type::Array { elem, .. } => Some(self.array_class(elem)?),
            _ => None,
        })
    }

    pub fn array_class(&mut self, elem: TypeId) -> R<ClassId> {
        let name = match self.world.pool.ty(elem) {
            Type::Bool => "BooleanArray",
            Type::Char => "CharArray",
            Type::Byte => "ByteArray",
            Type::Short => "ShortArray",
            Type::Int => "IntArray",
            Type::Long => "LongArray",
            Type::Float => "FloatArray",
            Type::Double => "DoubleArray",
            _ => "ObjectArray",
        };
        self.link_top(&format!("scala.scalanative.runtime.{name}"))
    }

    pub fn is_allocated(&self, cls: ClassId) -> bool {
        self.allocated_set.contains(&cls)
    }

    pub fn allocate(&mut self, cls: ClassId) {
        if self.allocated_set.insert(cls) {
            self.allocated.push(cls);
        }
    }

    /// Reaches function `g`; whether it was new.
    pub fn func(&mut self, g: GlobalId) -> R<bool> {
        if self.funcs.contains(&g) {
            return Ok(false);
        }
        let defn = self.world.defn(g).map_err(|e| e.to_string())?;
        if !matches!(defn.kind, DefnKind::Define { .. }) {
            return Err(format!("{} is not defined here", self.world.name(g)));
        }
        self.funcs.insert(g);
        self.queue.push(g);
        Ok(true)
    }

    /// Reaches module `cls` and its constructor; its slot.
    pub fn module(&mut self, cls: ClassId) -> R<usize> {
        if let Some(slot) = self.modules.iter().position(|&m| m == cls) {
            return Ok(slot);
        }
        self.modules.push(cls);
        self.allocate(cls);
        if let Some(ctor) = self.constructor(cls) {
            self.func(ctor)?;
        }
        Ok(self.modules.len() - 1)
    }

    /// The no-argument constructor of `cls`, if it has one.
    pub fn constructor(&mut self, cls: ClassId) -> Option<GlobalId> {
        let ctor = self.world.member(self.classes.get(cls).name, "RE");
        self.world.defn(ctor).ok().filter(|d| matches!(d.kind, DefnKind::Define { .. })).map(|_| ctor)
    }

    /// The implementations of called methods in allocated classes not reached before.
    pub fn dispatch_targets(&mut self) -> R<Vec<GlobalId>> {
        let mut out = Vec::new();
        let sigs: Vec<StrId> = self.vcalls.union(&self.icalls).copied().collect();
        for i in 0..self.allocated.len() {
            let cls = self.allocated[i];
            for &sig in &sigs {
                if self.dispatched.insert((cls, sig))
                    && let Some(g) = self.classes.resolve(self.world, cls, sig)?
                {
                    out.push(g);
                }
            }
        }
        Ok(out)
    }
}
