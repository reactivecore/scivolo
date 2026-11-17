// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//! The annotations of annotated types: their classes, a capture set as its type (dotty's
//! `CompactAnnotation`).

use super::Decoder;
use crate::NodeId;
use crate::tags::*;
use crate::types::{Type, TypeId};
use names::nme;

/// The package of capture checking's annotation classes.
const ANNOTATION_PACKAGE: &str = "scala.annotation";

impl Decoder<'_> {
    /// Decodes the annotation of the annotated type at `id`: a capture set as its type, another
    /// annotation as its class.
    pub fn annotation(&mut self, id: NodeId) -> TypeId {
        let annotation = self.ast.kids(id).get(1).and_then(|&a| {
            self.retaining(a).or_else(|| self.annotation_class(a).map(|class| self.ty(class)))
        });
        annotation.unwrap_or_else(|| self.intern(Type::Other(self.ast.tag(id))))
    }

    /// The class tree of the annotation term at `id`: what its constructor call instantiates.
    fn annotation_class(&self, id: NodeId) -> Option<NodeId> {
        let ast = self.ast;
        let id = ast.deref(id);
        match ast.tag(id) {
            NEW => ast.kids(id).first().copied(),
            _ => ast.kids(id).iter().find_map(|&k| self.annotation_class(k)),
        }
    }

    /// Decodes annotation tree `tree`, a constructor call of `scala.annotation.retains`,
    /// `retainsByName` or `retainsCap`, as the class applied to the call's type arguments:
    /// `retains[C]` of `new retains[C]()` (dotty's `CompactAnnotation.apply(tree)`). `None` for
    /// another annotation or a call with term arguments.
    fn retaining(&mut self, tree: NodeId) -> Option<TypeId> {
        let ast = self.ast;
        let apply = ast.deref(tree);
        if ast.tag(apply) != APPLY {
            return None;
        }
        let &[fun] = ast.kids(apply) else { return None };

        let fun = ast.deref(fun);
        let (select, targs) = match ast.tag(fun) {
            TYPEAPPLY => (ast.deref(*ast.kids(fun).first()?), &ast.kids(fun)[1..]),
            _ => (fun, &[][..]),
        };
        let &[new, owner] = ast.kids(select) else { return None };
        if ast.tag(select) != SELECTin || ast.tag(ast.deref(new)) != NEW {
            return None;
        }

        let class = self.ty(owner);
        if !self.is_retains_like(class) {
            return None;
        }
        let args: Vec<TypeId> = targs.iter().map(|&a| self.ty(a)).collect();
        Some(match args.is_empty() {
            true => class,
            false => self.intern_applied(class, &args),
        })
    }

    /// Whether annotation type `annot` is of class `scala.annotation.retains`, `retainsByName` or
    /// `retainsCap`.
    pub fn is_retains_like(&self, annot: TypeId) -> bool {
        let class = match *self.types.get(annot) {
            Type::Applied(tycon, _) => tycon,
            _ => annot,
        };
        let Type::TypeRef(prefix, name) = *self.types.get(class) else { return false };
        let package = self.packages.get(ANNOTATION_PACKAGE);
        matches!(self.types.get(prefix), Type::Package(p) if Some(p) == package)
            && matches!(self.types.name(name), nme::RETAINS | nme::RETAINS_BY_NAME | nme::RETAINS_CAP)
    }
}
