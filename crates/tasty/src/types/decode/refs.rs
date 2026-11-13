// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! References to definitions and packages.

use super::Decoder;
use crate::NodeId;
use crate::tags::*;
use crate::types::{Binder, Type};
use names::nme;

impl Decoder<'_> {
    /// Decodes the `REFINEDtpt` at `id`: its parent refined by its members, a recursive type if a
    /// member refers to another (dotty's `assignType` of a `RefinedTypeTree`, `RecType.closeOver`).
    pub fn refined_tpt(&mut self, id: NodeId) -> Type {
        let ast = self.ast;
        let (binder, kids) = (self.binder(id), ast.kids(id));
        let outer = self.selves.len();
        self.selves.extend(kids[1..].iter().map(|&m| (m, binder, false)));
        self.refining.push(id);
        let t = self.refined(kids[0], &kids[1..]);
        self.refining.pop();
        let recursive = self.selves[outer..].iter().any(|&(_, _, used)| used);
        self.selves.truncate(outer);
        match recursive {
            true => Type::Rec(binder, t),
            false => *self.types.get(t),
        }
    }

    /// Decodes a reference to member `def` of a refinement being decoded, through its recursive
    /// `this`.
    fn self_ref(&mut self, def: NodeId, term: bool) -> Option<Type> {
        let entry = self.selves.iter_mut().rev().find(|(m, _, _)| *m == def)?;
        entry.2 = true;
        let this = self.types.intern(Type::RecThis(entry.1));
        let name = self.name(def);
        Some(if term { Type::TermRef(this, name) } else { Type::TypeRef(this, name) })
    }

    /// A `TYPEREFdirect`, `TERMREFdirect`, `TYPEREFsymbol` or `TERMREFsymbol` at `id`.
    pub fn symbol_ref(&mut self, id: NodeId) -> Type {
        let ast = self.ast;
        let n = ast.node(id);
        let Some(def) = ast.at(n.a as u32) else { return Type::Other(n.tag) };
        let renamed = ast.kids(id).first().is_some_and(|&p| self.is_jdk_renamed(p, def));
        let prefix = (matches!(n.tag, TYPEREFsymbol | TERMREFsymbol) && !renamed).then(|| self.kid(id, 0));
        if let Some((binder, i)) = self.capture(def) {
            return Type::Param(binder, i);
        }
        if let Some(t) = self.self_ref(def, matches!(n.tag, TERMREFdirect | TERMREFsymbol)) {
            return t;
        }
        match self.by_tree.get(&(self.file, def)) {
            Some(&s) => Type::Sym(prefix, s),
            None => Type::Local(Binder(self.file, n.a as u32), self.name(def)),
        }
    }

    /// A `TYPEREFpkg` or `TERMREFpkg` at `id`.
    pub fn package_ref(&mut self, id: NodeId) -> Type {
        let name = self.text(id);
        let name = name.strip_prefix(nme::ROOT_PKG).map_or(name, |n| n.trim_start_matches('.'));
        let key = if name == nme::EMPTY_PACKAGE { "" } else { name };
        match self.packages.get(key) {
            Some(&p) => Type::Package(p),
            None => Type::PackageName(self.types.intern_name(name)),
        }
    }

    /// Whether definition `def`, seen through the package path or `this` at `prefix`, is a JDK
    /// class Scala Native defines under another name, entered in its JDK package.
    fn is_jdk_renamed(&self, prefix: NodeId, def: NodeId) -> bool {
        let ast = self.ast;
        let mut p = ast.deref(prefix);
        if ast.tag(p) == THIS
            && let Some(&k) = ast.kids(p).first()
        {
            p = ast.deref(k);
        }
        matches!(ast.tag(p), TYPEREFpkg | TERMREFpkg)
            && crate::symbols::jdk_name(self.text(p), self.text(def)).is_some()
    }
}
