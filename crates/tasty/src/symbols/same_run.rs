//! The library classes a library build compiles in the same run as its sources.

impl super::Universe {
    /// Makes the classes of jar `jar` (an index) compiled in the same run as the sources, as when
    /// the sources are scalalib's and the jar Scala's library.
    pub fn set_same_run_jar(&mut self, jar: usize) {
        self.same_run_jar = Some(jar);
    }

    /// Whether symbol `s` from TASTy is compiled in the same run as the sources: any when
    /// javalib's sources are compiled, else one of the jar `set_same_run_jar` names.
    pub fn is_same_run(&self, s: crate::SymbolId) -> bool {
        let Some((file, _)) = self.symbol(s).tree else { return false };
        if self.javalib_sources {
            return true;
        }
        let jar = self.same_run_jar.and_then(|_| self.library.jar_of(&self.files[file.0 as usize].path));
        jar.is_some() && jar == self.same_run_jar
    }
}
