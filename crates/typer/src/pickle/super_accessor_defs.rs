// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The definitions of super accessors, without bodies (`DefDef(acc, EmptyTree)`).

use super::{Refusal, TreePickler};
use tasty::tags::{ARTIFACT, DEFDEF, EMPTYCLAUSE, PARAM, SPLITCLAUSE, TYPEPARAM};
use tasty::write::Name;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl TreePickler<'_> {
    /// Super accessor `acc`, declared without a body.
    pub fn super_accessor_def(&mut self, acc: SymbolId) -> Result<(), Refusal> {
        self.defs.entry(acc).or_insert(None);
        self.register_def(acc);
        let name = self.accessor_name(acc).ok_or("a super accessor without a name")?;
        let info = self.u.info(acc);
        self.with_length(DEFDEF, &mut |p| {
            p.name(&name);
            let mut t = p.accessor_type_params(acc, info)?;
            let mut term_before = false;
            while let Type::Method { params, result, implicit, contextual, .. } = p.u.types().get(t).clone() {
                if implicit || contextual {
                    return Err("a super call of a method with implicit parameters");
                }
                if params.is_empty() {
                    p.byte(EMPTYCLAUSE);
                } else if term_before {
                    p.byte(SPLITCLAUSE);
                }
                for (name, tpe) in params.iter() {
                    p.with_length(PARAM, &mut |p| {
                        p.name(&Name::utf8(name));
                        p.ty(*tpe, false)
                    })?;
                }
                (t, term_before) = (result, !params.is_empty());
            }
            if matches!(p.u.types().get(t), Type::Poly { .. }) {
                return Err("a super call of a method with type parameters after its parameters");
            }
            p.ty(t, false)?;
            p.byte(ARTIFACT);
            Ok(())
        })
    }

    /// The type parameters of super accessor `acc` of type `info`, a type parameter for each of
    /// a polymorphic type's (dotty's `polyDefDef`); and the method type they are of.
    fn accessor_type_params(&mut self, acc: SymbolId, info: TypeId) -> Result<TypeId, Refusal> {
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else { return Ok(info) };
        let mut syms = Vec::new();
        for (name, _) in &params {
            let symbol = Symbol {
                name: name.clone(),
                kind: Kind::TypeParam,
                owner: None,
                flags: Flags::default(),
                target: None,
                is_extern: false,
                derived: false,
                decls: Vec::new(),
                tree: None,
            };
            let s = self.u.define(symbol);
            self.u.set_owner(s, acc);
            self.defs.insert(s, None);
            syms.push(s);
        }
        self.method_binders.insert(binder, syms.clone());
        for (s, (name, bounds)) in syms.into_iter().zip(params) {
            self.register_def(s);
            self.with_length(TYPEPARAM, &mut |p| {
                p.name(&Name::utf8(&name));
                p.ty(bounds, false)
            })?;
        }
        Ok(result)
    }
}
