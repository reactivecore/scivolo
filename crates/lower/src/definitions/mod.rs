//! The library symbols the lowering refers to by name (dotty's `Definitions`, nscplugin's
//! `NirDefinitions`): those of the builtin classes up front, the others on first use.

mod case_classes;
mod defined;
mod equality;
mod interfaces;
mod known;
mod lazy_vals;
mod monitors;
mod partial_functions;
mod patmat;
mod primitives;
mod refs;
mod resolved;
mod sequences;
mod strings;
mod tuples;

pub use case_classes::CaseClasses;
pub use defined::symbol;
use defined::{java_class, java_static};
pub use equality::Equality;
pub use interfaces::{procedure, specialized, specialized_applies, specialized_apply};
pub use known::Known;
pub use lazy_vals::LazyVals;
pub use monitors::Monitors;
pub use partial_functions::PartialFunctions;
pub use patmat::Patmat;
pub use primitives::{Primitive, boxed_class, primitive_class_name};
pub use refs::{Ref, ref_class};
pub use resolved::Serialization;
pub use sequences::Sequences;
use std::cell::Cell;
pub use strings::Strings;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};
pub use tuples::{specialized_accessor, specialized_tuple_ctor, tuple_tags};

pub struct Definitions {
    /// `scala.Boolean`.
    pub boolean: SymbolId,
    /// `Any.asInstanceOf`, `Any.isInstanceOf`, and `Any.$isInstanceOf$` that pattern matches
    /// test with.
    pub as_instance_of: SymbolId,
    pub is_instance_of: SymbolId,
    pub type_test: SymbolId,
    /// `Any.$asInstanceOf$`, the cast the compiler inserts.
    pub type_cast: SymbolId,
    /// `Any.==`, `Any.!=`, `Object.ne`, `Object.eq`, `Object.synchronized`, and `Object.clone`
    /// that erasure calls instead of `Array.clone`.
    pub any_eq: SymbolId,
    pub any_ne: SymbolId,
    pub ne: SymbolId,
    pub eq: SymbolId,
    pub synchronized: SymbolId,
    pub object_clone: SymbolId,
    /// `Boolean.&&`.
    pub and: SymbolId,
    /// `scala.runtime.BoxedUnit.UNIT`, a Java static field.
    pub boxed_unit_unit: SymbolId,
    /// The library's `BoxedUnit.UNIT` if it is a method.
    pub boxed_unit_method: Option<SymbolId>,
    /// dotty's `<special-ops>.throw`.
    pub throw: SymbolId,
    primitives: [Option<Primitive>; 8],
    equality: Option<Equality>,
    strings: Option<Strings>,
    patmat: Option<Patmat>,
    case_classes: Option<CaseClasses>,
    lazy_vals: Option<LazyVals>,
    monitors: Option<Monitors>,
    sequences: Option<Sequences>,
    serialization: Option<Serialization>,
    partial_functions: Option<PartialFunctions>,
    product: Option<SymbolId>,
    scala_run_time: Option<SymbolId>,
    array_clone: Option<SymbolId>,
    known: [Cell<Option<SymbolId>>; known::COUNT],
}

impl Definitions {
    /// Looks up the definitions the lowering needs up front, after entering `Predef`, `Unit` and
    /// `String` into `Defn`; the others are left to their first use.
    pub fn new(u: &mut Universe) -> Result<Definitions, String> {
        let b = u.enter_library_defn()?;
        let runtime = package(u, "scala.runtime")?;
        let boolean = member(u, b.scala, "Boolean", Kind::Class)?;
        Ok(Definitions {
            boolean,
            as_instance_of: member(u, b.any, "asInstanceOf", Kind::Def)?,
            is_instance_of: member(u, b.any, "isInstanceOf", Kind::Def)?,
            type_test: member(u, b.any, "$isInstanceOf$", Kind::Def)?,
            type_cast: member(u, b.any, "$asInstanceOf$", Kind::Def)?,
            any_eq: member(u, b.any, "==", Kind::Def)?,
            any_ne: member(u, b.any, "!=", Kind::Def)?,
            ne: member(u, b.object, "ne", Kind::Def)?,
            eq: member(u, b.object, "eq", Kind::Def)?,
            synchronized: member(u, b.object, "synchronized", Kind::Def)?,
            object_clone: member(u, b.object, "clone", Kind::Def)?,
            and: member(u, boolean, "&&", Kind::Def)?,
            boxed_unit_unit: defined::boxed_unit_unit(u, runtime),
            boxed_unit_method: member(u, runtime, "BoxedUnit$", Kind::Class)
                .and_then(|statics| member(u, statics, "UNIT", Kind::Def))
                .ok(),
            throw: patmat::throw(u)?,
            primitives: [None; 8],
            equality: None,
            strings: None,
            patmat: None,
            case_classes: None,
            lazy_vals: None,
            monitors: None,
            sequences: None,
            serialization: None,
            partial_functions: None,
            product: None,
            scala_run_time: None,
            array_clone: None,
            known: Default::default(),
        })
    }

    /// Whether `s` is `BoxedUnit.UNIT` (dotty's `BoxedUnit_UNIT`).
    pub fn is_boxed_unit(&self, s: SymbolId) -> bool {
        s == self.boxed_unit_unit || Some(s) == self.boxed_unit_method
    }
}

fn package(u: &Universe, name: &str) -> Result<SymbolId, String> {
    u.package(name).ok_or_else(|| format!("package {name} missing from the library"))
}

fn member(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind) -> Result<SymbolId, String> {
    u.library_member(owner, name, kind)
}

/// `class[args]`.
pub fn applied(u: &mut Universe, class: SymbolId, args: Vec<TypeId>) -> TypeId {
    let tycon = type_ref(u, class);
    u.intern_applied(tycon, &args)
}

/// The type of a reference to `s` from where its owner's members are visible.
pub fn type_ref(u: &mut Universe, s: SymbolId) -> TypeId {
    let prefix = match u.symbol(s).owner {
        Some(o) if u.symbol(o).kind == Kind::Package => Some(u.intern_type(Type::Package(o))),
        Some(o) if u.symbol(o).kind == Kind::Class => {
            let class = type_ref(u, o);
            Some(u.intern_type(Type::This(class)))
        }
        _ => None,
    };
    u.intern_type(Type::Sym(prefix, s))
}
