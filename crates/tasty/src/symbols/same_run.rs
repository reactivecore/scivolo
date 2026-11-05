//! The library classes a library build compiles in the same run as its sources.

impl super::Universe {
    /// Makes the classes of jar `jar` (an index) compiled in the same run as the sources, as when
    /// the sources are scalalib's and the jar Scala's library.
    pub fn set_same_run_jar(&mut self, jar: usize) {
        self.same_run_jar = Some(jar);
    }

    /// Whether the sources are Scala's library (dotty's `-Ycompile-scala2-library`).
    pub fn compiles_scala_library(&self) -> bool {
        self.same_run_jar.is_some()
    }

    /// Whether symbol `s` from TASTy is compiled in the same run as the sources: any when
    /// javalib's sources are compiled, else one of the jar `set_same_run_jar` names.
    pub fn is_same_run(&self, s: crate::SymbolId) -> bool {
        let Some((file, _)) = self.symbol(s).tree else { return false };
        if self.javalib_sources {
            return true;
        }
        let path = &self.files[file.0 as usize].path;
        self.same_run_jar.is_some_and(|jar| self.library.jar_of(path) == Some(jar))
    }
}
