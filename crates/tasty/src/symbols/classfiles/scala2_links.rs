//! The class files of the Scala 2 classes of jars, against which code links to their members
//! and classes extend them (ADR 0128).

use super::super::encoded_names::encode;
use super::super::{Kind, SymbolId, Universe};
use classfile::{ACC_ABSTRACT, ACC_PRIVATE, ClassInfo};

/// The separator of the name of a trait setter, `T$_setter_$x_$eq` (dotty's `TraitSetterName`).
const TRAIT_SETTER: &str = "$_setter_$";

impl Universe {
    /// Reads the class file of internal name `internal` from the jars, also one holding a Scala 2
    /// pickle or a compilation artifact of Scala 2.
    pub fn scala2_class_file(&mut self, internal: &str) -> Option<ClassInfo> {
        let jar = self.java_classes.as_ref()?.jar_of(internal)?;
        let bytes = self.library.read_class(jar, internal).ok()?;
        classfile::read(&bytes).ok()
    }

    /// Checks that a call of method `m` of a Scala 2 class of a jar, through class file
    /// `internal` (its class's, or its object's `C$`) as `name` of descriptor `desc`, links: the
    /// class file declares that method. A member of another class passes. Refuses a call whose
    /// erasure the class file lacks, so that no erasure difference reaches the bytecode.
    pub fn check_scala2_link(
        &mut self,
        m: SymbolId,
        internal: &str,
        name: &str,
        desc: &str,
    ) -> Result<(), String> {
        let Some(owner) = self.symbol(m).owner.filter(|&o| self.is_pickled(o)) else { return Ok(()) };
        let file = self.scala2_class_file(internal);
        let declared = file.is_some_and(|f| f.methods.iter().any(|d| d.name == name && d.descriptor == desc));
        if declared {
            return Ok(());
        }
        let member = format!("{}.{}", self.full_name(owner), self.symbol(m).name);
        Err(format!(
            "Scala 2 member `{member}` of a jar: erased as `{name}{desc}`, not in the class file `{internal}`"
        ))
    }

    /// Determines an abstract method of the class file of Scala 2 class `c` of a jar that its
    /// pickle does not explain, which a class extending `c` would leave unimplemented: one
    /// whose name is no member of `c` nor the trait setter of one, as the getters of private
    /// values and super accessors.
    pub fn scala2_unexplained_abstract(&mut self, c: SymbolId) -> Option<String> {
        let internal = self.full_name(c).replace('.', "/");
        let file = self.scala2_class_file(&internal)?;
        let decls = self.symbol(c).decls.clone();
        let names: Vec<String> = decls.iter().map(|&d| encode(&self.symbol(d).name)).collect();
        let values: Vec<&String> = names
            .iter()
            .zip(&decls)
            .filter(|&(_, &d)| self.symbol(d).kind == Kind::Val)
            .map(|(n, _)| n)
            .collect();
        let explained = |m: &str| {
            names.iter().any(|n| n == m)
                || m.split_once(TRAIT_SETTER)
                    .is_some_and(|(_, v)| values.iter().any(|n| format!("{n}_$eq") == v))
        };
        let abstract_ =
            file.methods.iter().filter(|m| m.access & ACC_ABSTRACT != 0 && m.access & ACC_PRIVATE == 0);
        abstract_.map(|m| m.name.clone()).find(|m| !explained(m))
    }
}
