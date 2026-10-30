//! References to definitions and packages.

use super::Decoder;
use crate::NodeId;
use crate::tags::*;
use crate::types::{Binder, Type};

impl Decoder<'_> {
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
        match self.by_tree.get(&(self.file, def)) {
            Some(&s) => Type::Sym(prefix, s),
            None => Type::Local(Binder(self.file, n.a as u32), self.name(def)),
        }
    }

    /// A `TYPEREFpkg` or `TERMREFpkg` at `id`.
    pub fn package_ref(&self, id: NodeId) -> Type {
        let name = self.name(id);
        let name = name.strip_prefix("_root_").map_or(name.as_str(), |n| n.trim_start_matches('.'));
        let key = if name == "<empty>" { "" } else { name };
        match self.packages.get(key) {
            Some(&p) => Type::Package(p),
            None => Type::PackageName(name.to_owned()),
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
            && crate::symbols::jdk_name(&self.name(p), &self.name(def)).is_some()
    }
}
