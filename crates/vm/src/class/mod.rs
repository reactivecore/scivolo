//! Linked classes, traits and modules: hierarchy and instance layout.

mod fields;

use crate::interp::Unwound;
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, Type, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

pub type ClassId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Class,
    Module,
    Trait,
}

pub struct Field {
    pub offset: u32,
    pub ty: TypeId,
}

pub struct Class {
    pub name: GlobalId,
    pub kind: Kind,
    pub parent: Option<ClassId>,
    pub traits: Vec<ClassId>,
    pub ancestors: FxHashSet<ClassId>,
    pub fields: FxHashMap<GlobalId, Field>,
    pub field_order: Vec<GlobalId>,
    pub size: u32,
    pub array_elem: Option<u32>,
    pub rtti: usize,
}

struct InstanceLayout {
    fields: FxHashMap<GlobalId, Field>,
    order: Vec<GlobalId>,
    size: u32,
}

#[derive(Default)]
pub struct Classes {
    pub all: Vec<Class>,
    by_name: FxHashMap<GlobalId, ClassId>,
    pub vtables: FxHashMap<(ClassId, nir::StrId), crate::interp::FuncId>,
}

const ARRAYS: [(&str, u32); 10] = [
    ("BooleanArray", 1),
    ("CharArray", 2),
    ("ByteArray", 1),
    ("ShortArray", 2),
    ("IntArray", 4),
    ("LongArray", 8),
    ("FloatArray", 4),
    ("DoubleArray", 8),
    ("ObjectArray", 8),
    ("BlobArray", 1),
];

impl Vm {
    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes.all[id as usize]
    }

    /// The linked class named `name`, linking it and its ancestors on first use.
    pub fn link(&mut self, name: GlobalId) -> Result<ClassId, VmError> {
        if let Some(&id) = self.classes.by_name.get(&name) {
            return Ok(id);
        }
        let defn = self.world.defn(name)?;
        let (kind, parent, traits) = match &defn.kind {
            DefnKind::Class { parent, traits } => (Kind::Class, *parent, traits.clone()),
            DefnKind::Module { parent, traits } => (Kind::Module, *parent, traits.clone()),
            DefnKind::Trait { traits } => (Kind::Trait, None, traits.clone()),
            _ => return Err(VmError::Unsupported(format!("{} is not a class", self.world.name(name)))),
        };
        let parent = parent.map(|p| self.link(p)).transpose()?;
        let traits = traits.into_iter().map(|t| self.link(t)).collect::<Result<Vec<_>, _>>()?;
        let id = self.classes.all.len() as ClassId;
        let mut ancestors = FxHashSet::from_iter([id]);
        for &a in parent.iter().chain(&traits) {
            ancestors.extend(self.class(a).ancestors.iter().copied());
        }
        let array_elem = array_elem(self.world.pool.top_name(name));
        let InstanceLayout { fields, order: field_order, size } =
            self.field_layout(name, parent, array_elem.is_some())?;
        self.classes.all.push(Class {
            name,
            kind,
            parent,
            traits,
            ancestors,
            fields,
            field_order,
            size,
            array_elem,
            rtti: 0,
        });
        self.classes.by_name.insert(name, id);
        self.init_rtti(id)?;
        self.run_static_initializer(name)?;
        Ok(id)
    }

    /// Runs the `<clinit>` of class `name`, if it has one.
    fn run_static_initializer(&mut self, name: GlobalId) -> Result<(), VmError> {
        let clinit = self.world.member(name, "IE");
        if self.world.defn(clinit).is_err() {
            return Ok(());
        }
        match self.call_global(clinit, &[]) {
            Ok(_) => Ok(()),
            Err(u) => match u.into_inner() {
                Unwound::Throw(exc) => Err(VmError::Initializer(exc)),
                Unwound::Fatal(e) => Err(e),
            },
        }
    }

    /// Links the class that owns member `g`, running its static initializer on first use.
    pub fn link_owner(&mut self, g: GlobalId) -> Result<(), VmError> {
        let owner = self.world.owner(g);
        match owner != g
            && matches!(self.world.defn(owner)?.kind, DefnKind::Class { .. } | DefnKind::Module { .. })
        {
            true => self.link(owner).map(|_| ()),
            false => Ok(()),
        }
    }

    pub fn link_top(&mut self, name: &str) -> Result<ClassId, VmError> {
        let g = self.world.top(name);
        self.link(g)
    }

    /// The array class holding elements of type `elem`.
    pub fn array_class(&mut self, elem: TypeId) -> Result<ClassId, VmError> {
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

    /// Whether an instance of `sub` is an instance of `sup`.
    pub fn is_subclass(&self, sub: ClassId, sup: ClassId) -> bool {
        self.class(sub).ancestors.contains(&sup)
    }

    /// The byte offset of field `name` within its owner's instances.
    pub fn field_offset(&mut self, name: GlobalId) -> Result<usize, VmError> {
        let owner = self.link(self.world.owner(name))?;
        match self.class(owner).fields.get(&name) {
            Some(f) => Ok(f.offset as usize),
            None => Err(VmError::Unsupported(format!("no field {}", self.world.name(name)))),
        }
    }
}

fn array_elem(name: &str) -> Option<u32> {
    let short = name.strip_prefix("scala.scalanative.runtime.")?;
    ARRAYS.iter().find(|(n, _)| *n == short).map(|&(_, size)| size)
}

pub fn array_names() -> impl Iterator<Item = &'static str> {
    ARRAYS.iter().map(|(n, _)| *n)
}
