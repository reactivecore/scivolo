// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/CodeGen.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! A class file's bytes with the `TASTY` attribute, the UUID of the class's TASTy, and the
//! `Scala` marker (dotty's `genTastyAndSetAttributes`, `pickleMarkerForeign`).

use classfile::{ACC_PUBLIC, ACC_STATIC, ClassFile};

/// A class file: the class's internal name, its bytes, the TASTy its `TASTY` attribute names and
/// whether it has a `public static void main(String[])`.
pub struct ClassBytes {
    pub name: String,
    pub bytes: Vec<u8>,
    pub tasty: Option<Vec<u8>>,
    pub main: bool,
}

impl ClassBytes {
    /// Class `cf` named `name` with the `TASTY` attribute of `tasty` if given and the `Scala`
    /// marker.
    pub fn new(name: String, cf: &mut ClassFile, tasty: Option<Vec<u8>>) -> Result<Self, String> {
        if let Some(tasty) = &tasty {
            let header =
                tasty::Tasty::parse(tasty).map_err(|e| format!("our TASTy unreadable: {e:?}"))?.header;
            cf.custom_attribute("TASTY", header.uuid.to_vec());
        }
        cf.custom_attribute("Scala", Vec::new());
        let main = cf.declares(ACC_PUBLIC | ACC_STATIC, "main", "([Ljava/lang/String;)V");
        Ok(ClassBytes { name, bytes: cf.bytes()?, tasty, main })
    }
}
