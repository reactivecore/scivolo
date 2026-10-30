use super::Decoder;
use crate::NodeId;
use crate::tags::{TERMREFpkg, THIS, TYPEREFpkg};

impl Decoder<'_> {
    /// Whether definition `def`, seen through the package path or `this` at `prefix`, is a JDK
    /// class Scala Native defines under another name, entered in its JDK package.
    pub fn is_jdk_renamed(&self, prefix: NodeId, def: NodeId) -> bool {
        let ast = self.ast;
        let mut p = ast.deref(prefix);
        if ast.tag(p) == THIS
            && let Some(&k) = ast.kids(p).first()
        {
            p = ast.deref(k);
        }
        let name = self.name(def);
        matches!(ast.tag(p), TYPEREFpkg | TERMREFpkg)
            && name.starts_with('_')
            && crate::symbols::jdk_name(&self.name(p), &name).is_some()
    }
}
