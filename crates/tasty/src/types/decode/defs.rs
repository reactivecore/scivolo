// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The types of definitions: a method's type from its parameter clauses, the members of a
//! refinement, the bounds of an opaque type.

use super::Decoder;
use crate::tags::*;
use crate::types::{Binder, Type, TypeId};
use crate::{NodeId, Parts, View, view};

impl Decoder<'_> {
    /// The type of the method with definition parts `parts`: its parameter clauses around its
    /// result type.
    pub fn def_type(&mut self, parts: &Parts) -> TypeId {
        let ast = self.ast;
        let result = match parts.body.first() {
            Some(&tpt) => self.ty(tpt),
            None => self.intern(Type::Other(DEFDEF)),
        };
        param_tree_clauses(ast, parts.params).into_iter().rev().fold(result, |result, clause| {
            let binder = Binder(self.file, ast.node(clause[0]).addr);
            let params = clause
                .iter()
                .filter(|&&p| matches!(ast.tag(p), PARAM | TYPEPARAM))
                .map(|&p| (self.name(p), self.param_type(p)))
                .collect();
            let t = match ast.tag(clause[0]) {
                TYPEPARAM => Type::Poly { binder, params, result },
                _ => {
                    let has =
                        |tag: u8| clause.iter().any(|&p| ast.kids(p).iter().any(|&m| ast.tag(m) == tag));
                    let contextual = has(GIVEN);
                    let implicit = contextual || has(IMPLICIT);
                    Type::Method { binder, params, result, implicit, contextual }
                }
            };
            self.intern(t)
        })
    }

    /// The type of parameter `p` in its method's type: annotated `@InlineParam` if it is an
    /// inline parameter (dotty's `adaptParamInfo`).
    fn param_type(&mut self, p: NodeId) -> TypeId {
        let t = self.kid(p, 0);
        if !self.ast.kids(p).iter().any(|&m| self.ast.tag(m) == INLINE) {
            return t;
        }
        let internal = match self.packages.get("scala.annotation.internal") {
            Some(&pkg) => Type::Package(pkg),
            None => Type::PackageName("scala.annotation.internal".into()),
        };
        let internal = self.intern(internal);
        let annot = self.intern(Type::TypeRef(internal, "InlineParam".into()));
        match self.types.get(t).clone() {
            Type::ByName(r) => {
                let r = self.intern(Type::Annotated(r, annot));
                self.intern(Type::ByName(r))
            }
            _ => self.intern(Type::Annotated(t, annot)),
        }
    }

    /// The type of the definition at `id`: a method's type, else the type of its first child
    /// (a value's type tree, a type's right-hand side or bounds).
    pub fn definition_type(&mut self, id: NodeId) -> TypeId {
        match view(self.ast, id) {
            View::DefDef { parts, .. } => self.def_type(&parts),
            _ => self.kid(id, 0),
        }
    }

    /// `parent` refined by the member definitions `members`.
    pub fn refined(&mut self, parent: NodeId, members: &[NodeId]) -> TypeId {
        let mut t = self.ty(parent);
        for &m in members {
            if matches!(self.ast.tag(m), VALDEF | DEFDEF | TYPEDEF) {
                let mut info = self.definition_type(m);
                if self.ast.tag(m) == TYPEDEF && !self.is_bounds_or_alias(info) {
                    info = self.intern(Type::Alias(info));
                }
                t = self.intern(Type::Refined(t, self.name(m), info));
            }
        }
        t
    }

    /// Whether type member info `t` is bounds or an alias, of a type lambda's result too.
    fn is_bounds_or_alias(&self, t: TypeId) -> bool {
        match self.types.get(t) {
            Type::Alias(_) | Type::Bounds(..) => true,
            Type::Lambda { result, .. } => matches!(self.types.get(*result), Type::Bounds(..)),
            _ => false,
        }
    }

    /// The bounds of the opaque type defined at `id`, when it states them.
    pub fn opaque_bounds(&mut self, id: NodeId) -> Option<(TypeId, TypeId)> {
        let ast = self.ast;
        let rhs = ast.deref(*ast.kids(id).first()?);
        let kids: Vec<NodeId> = ast.kids(rhs).iter().copied().filter(|&k| !is_modifier(ast.tag(k))).collect();
        match (ast.tag(rhs), &kids[..]) {
            (TYPEBOUNDStpt, [lo, hi, _, ..]) => Some((self.ty(*lo), self.ty(*hi))),
            _ => None,
        }
    }
}

/// The parameter clauses of a method's parameter trees: runs of type parameters, runs of value
/// parameters ended by a type parameter or `SPLITCLAUSE`, and empty clauses.
pub fn param_tree_clauses(ast: &crate::Ast, params: &[NodeId]) -> Vec<Vec<NodeId>> {
    let mut out: Vec<Vec<NodeId>> = Vec::new();
    let mut prev = None;
    for &p in params {
        let tag = ast.tag(p);
        match tag {
            EMPTYCLAUSE => out.push(vec![p]),
            SPLITCLAUSE => {}
            _ if prev == Some(tag) => out.last_mut().expect("a clause").push(p),
            _ => out.push(vec![p]),
        }
        prev = Some(tag);
    }
    out
}
