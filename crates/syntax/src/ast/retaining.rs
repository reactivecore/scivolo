// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/untpd.scala
//! Capture-set annotations `@retains[refs]` on types (dotty's `makeRetaining`).

use super::{Ast, Attrs, NodeId, TName, Tree};
use crate::names::Names;
use names::nme;

impl Ast {
    /// `parent @<annot>[r1 | ... | rN]` for capture-set annotation class `annot` of
    /// `scala.annotation`, `@<annot>[Nothing]` for no `refs`; `parent @retainsCap` for `annot`
    /// `retainsCap` (dotty's `makeRetaining`).
    pub fn make_retaining(
        &mut self,
        names: &mut Names,
        parent: NodeId,
        refs: &[NodeId],
        annot: &str,
    ) -> NodeId {
        let mut name = |text: &str, is_type| TName { name: names.intern(text), is_type };
        let (root, scala, annotation) =
            (name(nme::ROOT_PKG, false), name(nme::SCALA, false), name(nme::ANNOTATION, false));
        let (constr, init) = (name(annot, true), name(nme::CONSTRUCTOR, false));
        let (nothing, or) = (name("scala.Nothing", true), name("scala.|", true));
        let trefs = match refs {
            _ if annot == nme::RETAINS_CAP => None,
            [] => Some(self.node(Tree::TypedSplice { sym: nothing })),
            _ => {
                let singletons: Vec<NodeId> =
                    refs.iter().map(|&reference| self.node(Tree::SingletonTypeTree { reference })).collect();
                singletons.into_iter().reduce(|a, b| {
                    let tpt = self.node(Tree::TypedSplice { sym: or });
                    let args = self.list(&[a, b]);
                    self.node(Tree::AppliedTypeTree { tpt, args })
                })
            }
        };
        let root = self.node(Tree::Ident { name: root });
        let scala = self.node(Tree::Select { qualifier: root, name: scala });
        let package = self.node(Tree::Select { qualifier: scala, name: annotation });
        let tycon = self.node(Tree::Select { qualifier: package, name: constr });
        let targs = trefs.map(|t| self.list(&[t]));
        if let Some(args) = targs {
            // Positions `tycon` as in `New(AppliedTypeTree(tycon, args))`.
            self.node(Tree::AppliedTypeTree { tpt: tycon, args });
        }
        let new = self.node(Tree::New { tpt: tycon });
        let mut fun = self.node(Tree::Select { qualifier: new, name: init });
        if let Some(args) = targs {
            fun = self.node(Tree::TypeApply { fun, args });
        }
        let args = self.list(&[]);
        let annot_tree = self.node(Tree::Apply { fun, args });
        if trefs.is_some() {
            self.add_attrs(annot_tree, Attrs::RETAINS_ANNOT);
        }
        self.node(Tree::Annotated { arg: parent, annot: annot_tree })
    }
}
