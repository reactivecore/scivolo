use super::{Name, Names};
use std::sync::Arc;

/// The counters of the fresh names of each file of a unit, one per prefix (dotty's
/// `FreshNameCreator` of a compilation unit), and the `$N$` names made.
#[derive(Default)]
pub struct FreshNames {
    files: Counters,
    /// The counter of the `$N$` names of all files, renumbered per file after typing.
    unique_count: u32,
    pub uniques: crate::desugar::uniques::Uniques,
}

/// The counters of the prefixed fresh names of a unit at one point.
#[derive(Clone)]
pub struct FreshCounters(Counters);

/// The counters of each file, shared between copies until changed.
type Counters = Arc<Vec<Arc<FileCounters>>>;

/// The counter of each prefix of one file's fresh names, shared between copies until changed.
type FileCounters = rustc_hash::FxHashMap<String, u32>;

impl FreshNames {
    /// Makes `prefix` followed by the next number for it in the unit's first file: `_$1`.
    pub fn next(&mut self, names: &mut Names, prefix: &str) -> Name {
        self.next_in(0, names, prefix)
    }

    /// Makes `prefix`, each `<` and `>` replaced by `$`, followed by the next number for it in the
    /// unit's `file`th file (dotty's `UniqueNameKind.mkString` with `str.sanitize`).
    pub fn next_in(&mut self, file: usize, names: &mut Names, prefix: &str) -> Name {
        let prefix = prefix.replace(['<', '>'], "$");
        let n = Arc::make_mut(self.file_mut(file)).entry(prefix.clone()).or_insert(0);
        *n += 1;
        names.intern(&format!("{prefix}{n}"))
    }

    /// Copies the counters of the prefixed fresh names.
    pub fn counters(&self) -> FreshCounters {
        FreshCounters(self.files.clone())
    }

    /// Resets the counters of the prefixed fresh names to `c`.
    pub fn reset_counters(&mut self, c: FreshCounters) {
        self.files = c.0;
    }

    /// Raises each counter of the prefixed fresh names to at least its value in `c`.
    pub fn raise_counters(&mut self, c: FreshCounters) {
        if Arc::ptr_eq(&self.files, &c.0) {
            return;
        }
        let files = Arc::make_mut(&mut self.files);
        if files.len() < c.0.len() {
            files.resize_with(c.0.len(), Default::default);
        }
        for (file, counters) in files.iter_mut().zip(c.0.iter()) {
            if Arc::ptr_eq(file, counters) {
                continue;
            }
            let file = Arc::make_mut(file);
            for (prefix, &n) in counters.iter() {
                let own = file.entry(prefix.clone()).or_insert(0);
                *own = (*own).max(n);
            }
        }
    }

    /// Makes the next `$N$` (dotty's `UniqueName`).
    pub fn unique(&mut self, names: &mut Names) -> Name {
        self.unique_count += 1;
        let name = names.intern(&format!("${}$", self.unique_count));
        self.uniques.push(name);
        name
    }

    /// Takes the counters of `other`, a single file's, for the unit's `file`th file; raises the
    /// `$N$` counter to at least `other`'s and takes its `$N$` names.
    pub fn include(&mut self, file: usize, other: &FreshNames) {
        *self.file_mut(file) = other.files.first().cloned().unwrap_or_default();
        self.unique_count = self.unique_count.max(other.unique_count);
        self.uniques.include(&other.uniques);
    }

    /// The counters of the unit's `file`th file, to change.
    fn file_mut(&mut self, file: usize) -> &mut Arc<FileCounters> {
        let files = Arc::make_mut(&mut self.files);
        if files.len() <= file {
            files.resize_with(file + 1, Default::default);
        }
        &mut files[file]
    }
}
