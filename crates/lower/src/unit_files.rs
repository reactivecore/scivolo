//! The files of a unit's definitions, and the fresh names of the lowering counted per file.

use crate::Lower;
use crate::phases::walk::class_defs;
use syntax::source::Offset;
use tasty::SymbolId;
use typer::tpd::TNodeId;

impl Lower<'_> {
    /// Records the file of each top-level class of typed unit `root`, its files starting at
    /// offsets `starts` but the first.
    pub fn record_files(&mut self, root: TNodeId, starts: &[Offset]) {
        if starts.is_empty() {
            return;
        }
        for td in class_defs(self.tpd, root) {
            let Some(class) = self.tpd.sym(td) else { continue };
            let Some(owner) = self.u.symbol(class).owner else { continue };
            if self.u.symbol(owner).is_package() {
                let at = self.tpd.span(td).in_own_source().without_inlined_call().start();
                self.class_files.insert(class, starts.partition_point(|&s| s <= at));
            }
        }
    }

    /// Determines the index of the file of the unit `s` is defined in, by its top-level class.
    pub fn file_of(&self, s: SymbolId) -> usize {
        let mut s = Some(s);
        while let Some(sym) = s {
            if let Some(&file) = self.class_files.get(&sym) {
                return file;
            }
            s = self.u.symbol(sym).owner;
        }
        0
    }

    /// Makes `prefix` followed by the next number for it in the file `s` is defined in (dotty's
    /// `FreshNameCreator` of the compilation unit).
    pub fn fresh_name(&mut self, s: SymbolId, prefix: &str) -> String {
        let file = self.file_of(s);
        let name = self.fresh.next_in(file, self.names, prefix);
        self.names.text(name).to_owned()
    }
}
