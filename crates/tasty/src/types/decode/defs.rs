// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The types of definitions: a method's type from its parameter clauses, the members of a
//! refinement, the bounds of an opaque type.

use super::Decoder;
use crate::tags::*;
use crate::types::{Binder, Type, TypeId};
use crate::{NodeId, Parts, View, view};

/// The bit that sets the binder of a constructor's added empty clause apart from its first one's.
const EMPTY_CLAUSE_BINDER: u32 = 1 << 31;

impl Decoder<'_> {
    /// The type of the method with definition parts `parts`: its parameter clauses around its
    /// result type, a constructor's first clause an empty one if it would be implicit (dotty's
    /// `normalizeIfConstructor`).
    pub fn def_type(&mut self, parts: &Parts, constructor: bool) -> TypeId {
        let ast = self.ast;
        let result = match parts.body.first() {
            Some(&tpt) => self.ty(tpt),
            None => self.intern(Type::Other(DEFDEF)),
        };
        let clauses = param_tree_clauses(ast, parts.params);
        let has = |clause: &[NodeId], tag: u8| {
            clause.iter().any(|&p| ast.kids(p).iter().any(|&m| ast.tag(m) == tag))
        };
        let values = clauses.iter().position(|c| ast.tag(c[0]) != TYPEPARAM).unwrap_or(clauses.len());
        let leading_implicit =
            clauses.get(values).is_some_and(|c| ast.tag(c[0]) == PARAM && has(c, IMPLICIT));
        let empty_before = (constructor && leading_implicit).then_some(values);
        clauses.into_iter().enumerate().rev().fold(result, |result, (i, clause)| {
            let binder = Binder(self.file, ast.node(clause[0]).addr);
            let params = clause
                .iter()
                .filter(|&&p| matches!(ast.tag(p), PARAM | TYPEPARAM))
                .map(|&p| (self.name(p), self.param_type(p)))
                .collect();
            let t = match ast.tag(clause[0]) {
                TYPEPARAM => Type::Poly { binder, params, result },
                _ => {
                    let contextual = has(&clause, GIVEN);
                    let implicit = contextual || has(&clause, IMPLICIT);
                    Type::Method { binder, params, result, implicit, contextual }
                }
            };
            let t = self.intern(t);
            if empty_before != Some(i) {
                return t;
            }
            let binder = Binder(self.file, ast.node(clause[0]).addr | EMPTY_CLAUSE_BINDER);
            let empty =
                Type::Method { binder, params: Vec::new(), result: t, implicit: false, contextual: false };
            self.intern(empty)
        })
    }

    /// The type of parameter `p` in its method's type: annotated `@InlineParam` if it is an
    /// inline parameter, `@ErasedParam` if it is an erased one (dotty's `adaptParamInfo`).
    fn param_type(&mut self, p: NodeId) -> TypeId {
        let mut t = self.kid(p, 0);
        for (tag, annot) in [(INLINE, "InlineParam"), (ERASED, "ErasedParam")] {
            if self.ast.kids(p).iter().any(|&m| self.ast.tag(m) == tag) {
                t = self.annotated_param(t, annot);
            }
        }
        t
    }

    fn annotated_param(&mut self, t: TypeId, annot: &str) -> TypeId {
        let internal = match self.packages.get("scala.annotation.internal") {
            Some(&pkg) => Type::Package(pkg),
            None => Type::PackageName("scala.annotation.internal".into()),
        };
        let internal = self.intern(internal);
        let annot = self.intern(Type::TypeRef(internal, annot.into()));
        match *self.types.get(t) {
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
            View::DefDef { name, parts } => {
                let constructor = self.names.get(name.0 as usize).is_some_and(|n| n == "<init>");
                self.def_type(&parts, constructor)
            }
            _ => self.kid(id, 0),
        }
    }

    /// `parent` refined by the member definitions `members`, a method member's parameters
    /// referred to as parameters of its type.
    pub fn refined(&mut self, parent: NodeId, members: &[NodeId]) -> TypeId {
        let mut t = self.ty(parent);
        for &m in members {
            if matches!(self.ast.tag(m), VALDEF | DEFDEF | TYPEDEF) {
                let outer = self.captures.len();
                if let View::DefDef { parts, .. } = view(self.ast, m) {
                    for clause in param_tree_clauses(self.ast, parts.params) {
                        let binder = Binder(self.file, self.ast.node(clause[0]).addr);
                        let params =
                            clause.into_iter().filter(|&p| matches!(self.ast.tag(p), PARAM | TYPEPARAM));
                        let captured: Vec<_> =
                            params.enumerate().map(|(i, p)| (p, binder, i as u32)).collect();
                        self.captures.extend(captured);
                    }
                }
                let mut info = self.definition_type(m);
                self.captures.truncate(outer);
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

    /// The bounds of the opaque type defined at `id`, when it states them; type lambdas over
    /// both bounds for a parameterised one.
    pub fn opaque_bounds(&mut self, id: NodeId) -> Option<(TypeId, TypeId)> {
        let ast = self.ast;
        let rhs = ast.deref(*ast.kids(id).first()?);
        if ast.tag(rhs) != LAMBDAtpt {
            return self.bounds_with_alias(rhs);
        }
        let (body, params) = ast.kids(rhs).split_last()?;
        let (lo, hi) = self.bounds_with_alias(*body)?;
        let binder = self.binder(rhs);
        let params: Vec<_> = params.iter().map(|&p| (self.name(p), self.kid(p, 0))).collect();
        let lambda = |d: &mut Self, result| d.intern(Type::Lambda { binder, params: params.clone(), result });
        Some((lambda(self, lo), lambda(self, hi)))
    }

    /// The bounds of `TYPEBOUNDStpt` node `id` if it also states an alias.
    fn bounds_with_alias(&mut self, id: NodeId) -> Option<(TypeId, TypeId)> {
        let ast = self.ast;
        let kids: Vec<NodeId> = ast.kids(id).iter().copied().filter(|&k| !is_modifier(ast.tag(k))).collect();
        match (ast.tag(id), &kids[..]) {
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
