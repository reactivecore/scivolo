//! NIR types of erased types and method signatures (nscplugin's `NirGenType`).

use super::Gen;
use super::names::mapped;
use crate::symbols::{is_constructor, is_java_static};
use crate::{Result, unsupported};
use nir::{Type, TypeId};
use syntax::ast::Span;
use tasty::{Erased, SymbolId};

impl Gen<'_, '_> {
    pub fn erased(&mut self, e: &Erased) -> TypeId {
        let t = match e {
            Erased::Prim(c) => match c {
                'z' => Type::Bool,
                'c' => Type::Char,
                'b' => Type::Byte,
                's' => Type::Short,
                'i' => Type::Int,
                'j' => Type::Long,
                'f' => Type::Float,
                'd' => Type::Double,
                'R' => Type::Ptr,
                'w' => Type::Size,
                c => unreachable!("a primitive letter: {c}"),
            },
            Erased::Unit => Type::Unit,
            Erased::Nothing => Type::Nothing,
            Erased::Null => Type::Null,
            Erased::Class(name) => {
                let name = self.pool.top(&mapped(name));
                Type::Ref { name, exact: false, nullable: true }
            }
            Erased::Array(elem) => Type::Array { elem: self.erased(elem), nullable: true },
        };
        self.ty(t)
    }

    /// Fails unless a value of type `from` passes where `to` is expected without boxing,
    /// unboxing or a conversion.
    pub fn check_conforms(&mut self, from: TypeId, to: TypeId, span: Span) -> Result<()> {
        let is_ref = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. });
        let (f, t) = (self.pool.ty(from), self.pool.ty(to));
        let ok = f == t || *f == Type::Nothing || (is_ref(t) && (is_ref(f) || *f == Type::Null));
        if ok { Ok(()) } else { unsupported(span, "boxing, unboxing and primitive conversions") }
    }

    /// The NIR type of the erasure of `t`.
    pub fn gen_type(&mut self, t: tasty::TypeId) -> TypeId {
        let e = self.l.u.erase(t);
        self.erased(&e)
    }

    /// The reference type of class `c`.
    pub fn class_ref(&mut self, c: SymbolId) -> TypeId {
        let name = self.type_name(c);
        self.ty(Type::Ref { name, exact: false, nullable: true })
    }

    pub fn param_types(&mut self, m: SymbolId) -> Vec<TypeId> {
        let (params, _) = self.l.u.erased_method(m);
        params.iter().map(|p| self.erased(p)).collect()
    }

    pub fn result_type(&mut self, m: SymbolId) -> TypeId {
        let (_, result) = self.l.u.erased_method(m);
        self.erased(&result)
    }

    /// The function type of method `m`: the receiver unless called statically, the parameters,
    /// the result.
    pub fn method_sig(&mut self, m: SymbolId, statically: bool) -> TypeId {
        let mut args = Vec::new();
        if !statically && !is_java_static(self.l.u, m) {
            let owner = self.l.u.symbol(m).owner.expect("a method's owner");
            args.push(self.class_ref(owner));
        }
        args.extend(self.param_types(m));
        let ret = if is_constructor(self.l.u, m) { self.ty(Type::Unit) } else { self.result_type(m) };
        self.ty(Type::Function(args, ret))
    }
}
