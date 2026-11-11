// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The definitions of super accessors, without bodies (`DefDef(acc, EmptyTree)`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use names::nme;
use syntax::ast::Span;
use tasty::tags::{
    ANNOTATION, APPLY, ARTIFACT, DEFDEF, EMPTYCLAUSE, NEW, PARAM, SELECTin, SPLITCLAUSE, STRINGconst,
    TYPEPARAM,
};
use tasty::write::Name;
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId};

impl TreePickler<'_> {
    /// Super accessor `acc`, declared without a body.
    pub fn super_accessor_def(&mut self, acc: SymbolId) -> Result<(), Refusal> {
        self.defs.entry(acc).or_insert(None);
        self.register_def(acc);
        let name = self.accessor_name(acc).ok_or("a super accessor without a name")?;
        let info = self.u.info(acc);
        let (span, member) = self.accessor_spans.get(&acc).copied().unwrap_or((Span::NONE, Span::NONE));
        self.synthetic_here(span, None);
        self.with_length(DEFDEF, &mut |p| {
            p.name(&name);
            let mut t = p.accessor_type_params(acc, info)?;
            let mut term_before = false;
            while let Some((params, result, implicit, contextual)) =
                p.u.method(t).map(|m| (m.params.to_vec(), m.result, m.implicit, m.contextual))
            {
                if implicit || contextual {
                    return Err("a super call of a method with implicit parameters");
                }
                if params.is_empty() {
                    p.byte(EMPTYCLAUSE);
                } else if term_before {
                    p.byte(SPLITCLAUSE);
                }
                for &(name, tpe) in params.iter() {
                    p.with_length(PARAM, &mut |p| {
                        let name = Name::utf8(p.u.name(name));
                        p.name(&name);
                        p.ty(tpe, false)
                    })?;
                }
                (t, term_before) = (result, !params.is_empty());
            }
            if p.u.poly(t).is_some() {
                return Err("a super call of a method with type parameters after its parameters");
            }
            let tpt = p.synthetic.len();
            p.synthetic_here(span, None);
            p.ty(t, false)?;
            p.byte(ARTIFACT);
            if let Some(target) = p.u.symbol(acc).target.clone() {
                p.target_name_annotation(&target, member)?;
            }
            if tpt < p.synthetic.len() {
                let entry = p.synthetic.remove(tpt);
                p.synthetic.push(entry);
            }
            Ok(())
        })
    }

    /// The annotation `@targetName(target)` (dotty's `deriveTargetNameAnnotation`), its type and
    /// argument at the point of `member`'s definition.
    fn target_name_annotation(&mut self, target: &str, member: Span) -> Result<(), Refusal> {
        let class = self.u.package("scala.annotation").and_then(|p| self.u.decl(p, "targetName").ok());
        let class = class.and_then(|d| d.into_iter().find(|&c| self.u.is_class(c)));
        let class = class.ok_or("a target name without the class targetName")?;
        let init = self.u.decl(class, nme::CONSTRUCTOR).ok().and_then(|d| d.first().copied());
        let init = init.ok_or("a target name without the constructor of targetName")?;
        let class = type_ref(self.u, class);
        self.with_length(ANNOTATION, &mut |p| {
            p.ty(class, false)?;
            p.with_length(APPLY, &mut |p| {
                p.with_length(SELECTin, &mut |p| {
                    p.signed_name(init, true)?;
                    p.byte(NEW);
                    p.synthetic_here(member, None);
                    p.ty(class, false)?;
                    p.ty(class, false)
                })?;
                p.synthetic_here(member, None);
                p.byte(STRINGconst);
                p.name(&Name::utf8(target));
                Ok(())
            })
        })
    }

    /// The type parameters of super accessor `acc` of type `info`, a type parameter for each of
    /// a polymorphic type's (dotty's `polyDefDef`); and the method type they are of.
    fn accessor_type_params(&mut self, acc: SymbolId, info: TypeId) -> Result<TypeId, Refusal> {
        let Some((binder, params, result)) =
            self.u.poly(info).map(|b| (b.binder, b.params.to_vec(), b.result))
        else {
            return Ok(info);
        };
        let mut syms = Vec::new();
        for &(name, _) in &params {
            let symbol = Symbol {
                name: self.u.name(name).to_owned(),
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
                let name = Name::utf8(p.u.name(name));
                p.name(&name);
                p.ty(bounds, false)
            })?;
        }
        Ok(result)
    }
}
