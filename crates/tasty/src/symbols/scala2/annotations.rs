// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! Annotations of a pickle (dotty's `readSymbolAnnotation`, `readAnnotation`, the `ANNOTATEDtpe`
//! case of `readType`): their classes, without their arguments.

use super::super::Universe;
use super::format::{ANNOTINFO, SYMANNOT};
use super::{Sym, Unpickler};
use crate::types::{Type, TypeId};

impl Unpickler {
    /// Annotates the symbols this pickle entered with the classes of their `SYMANNOT` entries;
    /// an annotation whose class is not read is left out, as dotty reads it only when asked.
    pub fn symbol_annotations(&mut self, u: &mut Universe) -> Result<(), String> {
        for i in 0..self.index.len() {
            if self.tag(i)? != SYMANNOT {
                continue;
            }
            self.seek(i)?;
            let target = self.buf.nat()?;
            let class = self.buf.nat()?;
            let Some(Sym::Known(s)) = self.syms.get(&target).copied() else { continue };
            if !self.is_own(s) {
                continue;
            }
            let Ok(t) = self.ty(u, class) else { continue };
            if let Some(c) = u.class_symbol(t) {
                u.annotate(s, c);
            }
        }
        Ok(())
    }

    /// Reads an `ANNOTATEDtpe` body ending at `end`: its type annotated with the class of each
    /// `ANNOTINFO`, whose arguments are left out.
    pub fn annotated(&mut self, u: &mut Universe, end: usize) -> Result<TypeId, String> {
        let underlying = self.buf.nat()?;
        let annots = self.refs(end)?;
        let mut t = self.ty(u, underlying)?;
        for a in annots {
            let (tag, _) = self.seek(a)?;
            if tag != ANNOTINFO {
                return Err(format!("an annotation of tag {tag}"));
            }
            let class = self.buf.nat()?;
            let class = self.ty(u, class)?;
            t = u.types.intern(Type::Annotated(t, class));
        }
        Ok(t)
    }
}
