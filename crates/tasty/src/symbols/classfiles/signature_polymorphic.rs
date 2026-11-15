// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The signature polymorphic methods of `MethodHandle` and `VarHandle` (JLS 15.12.3), and their
//! instances at calls.

use super::super::{Symbol, SymbolId, Universe};
use crate::types::TypeId;
use classfile::{ACC_NATIVE, ACC_VARARGS, MemberInfo};
use rustc_hash::{FxHashMap, FxHashSet};

/// The classes whose native varargs methods are signature polymorphic (dotty's
/// `containsSignaturePolymorphic`).
const SIGNATURE_POLYMORPHIC_CLASSES: [&str; 2] =
    ["java.lang.invoke.MethodHandle", "java.lang.invoke.VarHandle"];

/// Whether method `m` of the class file of class `class` is signature polymorphic: native
/// varargs, of `MethodHandle` or `VarHandle`.
pub fn is_signature_polymorphic(u: &Universe, class: SymbolId, m: &MemberInfo) -> bool {
    m.access & (ACC_NATIVE | ACC_VARARGS) == ACC_NATIVE | ACC_VARARGS
        && SIGNATURE_POLYMORPHIC_CLASSES.contains(&u.full_name(class).as_str())
}

/// The signature polymorphic methods of a universe and their instances.
#[derive(Default)]
pub struct SignaturePolymorphic {
    methods: FxHashSet<SymbolId>,
    instances: FxHashMap<SymbolId, SymbolId>,
}

impl Universe {
    /// Records method `m`, entered from a class file, as signature polymorphic.
    pub fn set_signature_polymorphic(&mut self, m: SymbolId) {
        self.signature_polymorphic.methods.insert(m);
    }

    /// Whether `m` is a signature polymorphic method (dotty's `isSignaturePolymorphic`).
    pub fn is_signature_polymorphic(&self, m: SymbolId) -> bool {
        self.signature_polymorphic.methods.contains(&m)
    }

    /// Creates an instance of signature polymorphic method `m` of type `info`, not entered into
    /// its class (dotty's `retypeSignaturePolymorphicFn`).
    pub fn signature_polymorphic_instance(&mut self, m: SymbolId, info: TypeId) -> SymbolId {
        let s = self.symbol(m);
        let instance = Symbol {
            name: s.name.clone(),
            kind: s.kind,
            owner: s.owner,
            flags: s.flags,
            target: s.target.clone(),
            is_extern: false,
            derived: s.derived,
            decls: Vec::new(),
            tree: None,
        };
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(instance);
        self.infos.insert(id, info);
        self.signature_polymorphic.instances.insert(id, m);
        id
    }

    /// The signature polymorphic method `s` is an instance of, if it is one (dotty's
    /// `originalSignaturePolymorphic`).
    pub fn original_signature_polymorphic(&self, s: SymbolId) -> Option<SymbolId> {
        self.signature_polymorphic.instances.get(&s).copied()
    }
}
