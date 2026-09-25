//! Per-function call counts, collected when `SCALAI_PROFILE` is set.

use super::FuncId;
use crate::Vm;

#[derive(Default)]
pub struct Profile {
    pub on: bool,
    calls: Vec<u64>,
    fallback: rustc_hash::FxHashMap<String, u64>,
}

impl Profile {
    /// Counts an operation the lowering left to the NIR fallback, by operation kind.
    pub fn fallback(&mut self, op: &nir::Op) {
        let text = format!("{op:?}");
        let kind = text.split(['{', '(', ' ']).next().unwrap_or_default().to_owned();
        *self.fallback.entry(kind).or_default() += 1;
    }

    pub fn count(&mut self, f: FuncId) {
        let f = f as usize;
        if self.calls.len() <= f {
            self.calls.resize(f + 1, 0);
        }
        self.calls[f] += 1;
    }
}

impl Vm {
    /// The `n` most called functions with their call counts, most called first.
    pub fn profile_report(&self, n: usize) -> Vec<(u64, String)> {
        let calls = &self.funcs.profile.calls;
        let mut top: Vec<(u64, FuncId)> =
            calls.iter().enumerate().filter(|(_, c)| **c > 0).map(|(f, &c)| (c, f as FuncId)).collect();
        top.sort_unstable_by(|a, b| b.cmp(a));
        let mut report: Vec<(u64, String)> =
            top.into_iter().take(n).map(|(c, f)| (c, self.func_name(f))).collect();
        report.extend(self.funcs.profile.fallback.iter().map(|(k, &c)| (c, format!("fallback op {k}"))));
        report
    }
}
