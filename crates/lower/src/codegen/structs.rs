// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! `@struct` classes: their values C structs of the constructor's parameters, their fields
//! extracted, their definitions a class of methods only (nscplugin's `genStruct`).

use super::Gen;
use crate::phases::casts::class_named;
use crate::{Result, unsupported};
use nir::{Defn, DefnKind, Op, PosId, Type, TypeId, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

const STRUCT: &str = "scala.scalanative.runtime.struct";

impl Gen<'_, '_> {
    /// The type of the values of class `name`, a binary name, if a `@struct` class: a C struct of
    /// its constructor's parameters.
    pub fn struct_type(&mut self, name: &str) -> Option<TypeId> {
        if let Some(&t) = self.structs.get(name) {
            return t;
        }
        let class = class_named(self.l, name).filter(|&c| self.l.u.is_annotated(c, STRUCT));
        let ctor = class.and_then(|c| {
            let decls = self.l.u.decls(c).unwrap_or_default();
            decls.into_iter().find(|&d| self.l.u.is_constructor(d))
        });
        let t = ctor.map(|ctor| {
            let fields = self.param_types(ctor);
            self.ty(Type::StructValue(fields))
        });
        self.structs.insert(name.to_owned(), t);
        t
    }

    pub fn is_struct(&mut self, c: SymbolId) -> bool {
        let name = self.l.u.binary_name(c);
        self.struct_type(&name).is_some()
    }

    /// `@struct` class `td`, `sym`, at `pos`: a class without parents or fields, and its methods
    /// but the constructor.
    pub fn gen_struct(&mut self, td: TNodeId, sym: SymbolId, pos: PosId) -> Result<()> {
        let name = self.type_name(sym);
        self.defns.push(Defn {
            name,
            attrs: Vec::new(),
            pos,
            kind: DefnKind::Class { parent: None, traits: Vec::new() },
        });
        let Tree::TypeDef { rhs, .. } = *self.l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { body, .. } = *self.l.tpd.tree(rhs) else {
            return unsupported(self.l.tpd.span(td), "a type definition that is not a class");
        };
        self.lambdas = 0;
        let first = self.defns.len();
        for stat in self.l.tpd.items(body).to_vec() {
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } if self.l.tpd.sym(stat).is_some_and(|m| self.l.u.is_constructor(m)) => {}
                Tree::DefDef { .. } => {
                    let defn = self.gen_method(stat)?;
                    self.defns.push(defn);
                }
                Tree::ValDef { .. } | Tree::Import { .. } => {}
                _ => return unsupported(self.l.tpd.span(stat), "statements in a `@struct` class"),
            }
        }
        if !self.closures.is_empty() {
            return unsupported(self.l.tpd.span(td), "lambdas in a `@struct` class");
        }
        let existing: Vec<nir::GlobalId> = self.defns[first..].iter().map(|d| d.name).collect();
        let forwarders = self.static_method_forwarders(&existing)?;
        self.defns.extend(forwarders);
        Ok(())
    }

    /// Field `f` of struct `obj` of class `owner`: its value at its index among the class's fields.
    pub fn gen_struct_field(
        &mut self,
        owner: SymbolId,
        f: SymbolId,
        obj: ValId,
        span: Span,
    ) -> Result<ValId> {
        let decls = self.l.u.decls(owner).unwrap_or_default();
        let fields: Vec<SymbolId> = decls.into_iter().filter(|&d| self.is_field(d)).collect();
        let Some(index) = fields.iter().position(|&d| d == f) else {
            return unsupported(span, "a field of a `@struct` class that is not its own");
        };
        Ok(self.emit(Op::Extract { aggr: obj, indexes: vec![index as i32] }))
    }
}
