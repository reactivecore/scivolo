//! Linked classes, traits and modules: hierarchy and instance layout.

use crate::layout::{self, ARRAY_HEADER, OBJECT_HEADER};
use crate::{Vm, VmError};
use nir::{DefnKind, Global, GlobalId, Type, TypeId};
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
        let array_elem = array_elem(self.world.pool.str(top_str(&self.world.pool, name)));
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
        Ok(id)
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

    fn field_layout(
        &mut self,
        name: GlobalId,
        parent: Option<ClassId>,
        is_array: bool,
    ) -> Result<InstanceLayout, VmError> {
        let (mut fields, mut order, mut tys) = (FxHashMap::default(), Vec::new(), Vec::new());
        if let Some(p) = parent {
            for &f in &self.class(p).field_order {
                let field = &self.class(p).fields[&f];
                fields.insert(f, Field { offset: field.offset, ty: field.ty });
                order.push(f);
                tys.push(field.ty);
            }
        }
        for m in self.world.members(name)?.to_vec() {
            if let DefnKind::Var { ty, .. } = self.world.defn(m)?.kind {
                order.push(m);
                tys.push(ty);
            }
        }
        let header = if is_array { ARRAY_HEADER } else { OBJECT_HEADER };
        let ptr = self.world.pool.types.intern(Type::Ptr);
        let header_ty = TypeId(self.world.pool.types.intern(Type::ArrayValue(TypeId(ptr), header / 8)));
        let all: Vec<TypeId> = std::iter::once(header_ty).chain(tys.iter().copied()).collect();
        let (size, offsets) = layout::struct_layout(&self.world.pool, &all);
        for (i, &f) in order.iter().enumerate() {
            fields.insert(f, Field { offset: offsets[i + 1], ty: tys[i] });
        }
        Ok(InstanceLayout { fields, order, size })
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

fn top_str(pool: &nir::Pool, g: GlobalId) -> nir::StrId {
    match pool.global(g) {
        Global::Top(s) => *s,
        Global::Member(owner, _) => top_str(pool, *owner),
        Global::None => nir::StrId(0),
    }
}

fn array_elem(name: &str) -> Option<u32> {
    let short = name.strip_prefix("scala.scalanative.runtime.")?;
    ARRAYS.iter().find(|(n, _)| *n == short).map(|&(_, size)| size)
}

pub fn array_names() -> impl Iterator<Item = &'static str> {
    ARRAYS.iter().map(|(n, _)| *n)
}
