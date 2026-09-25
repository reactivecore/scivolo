//! Op histograms and native-interop usage over a set of decoded files.

use nir::demangle;
use nir::{Attr, Defn, DefnKind, Inst, Next, Op, Pool, Val, ValId};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Stats {
    kinds: BTreeMap<&'static str, usize>,
    ops: BTreeMap<&'static str, usize>,
    insts: usize,
    files: usize,
    files_raw: usize,
    methods: usize,
    methods_raw: usize,
    methods_stackalloc: usize,
    methods_extern_call: usize,
    methods_unwind: usize,
    extern_decls: usize,
    externs: BTreeMap<String, usize>,
}

const RAW_OPS: [&str; 5] = ["load", "store", "elem", "stackalloc", "ptr-conv"];

impl Stats {
    pub fn add_file(&mut self, pool: &Pool, defns: &[Defn]) {
        self.files += 1;
        let mut file_raw = false;
        for d in defns {
            *self.kinds.entry(kind_name(&d.kind)).or_default() += 1;
            let extern_member = pool.member_sig(d.name).is_some_and(|s| s.starts_with('C'));
            if let DefnKind::Declare { .. } = d.kind
                && (d.attrs.iter().any(Attr::is_extern) || extern_member)
            {
                self.extern_decls += 1;
            }
            let DefnKind::Define { body, .. } = &d.kind else { continue };
            self.methods += 1;
            let (mut raw, mut stackalloc, mut ext, mut unwind) = (false, false, false, false);
            for inst in &body.insts {
                self.insts += 1;
                let Inst::Let { op, unwind: next, .. } = inst else { continue };
                let name = op_name(op);
                *self.ops.entry(name).or_default() += 1;
                raw |= RAW_OPS.contains(&name);
                stackalloc |= name == "stackalloc";
                unwind |= !matches!(next, Next::None);
                if let Op::Call { ptr, .. } = op
                    && let Some(sym) = extern_symbol(pool, *ptr)
                {
                    ext = true;
                    *self.externs.entry(sym).or_default() += 1;
                }
            }
            file_raw |= raw;
            self.methods_raw += raw as usize;
            self.methods_stackalloc += stackalloc as usize;
            self.methods_extern_call += ext as usize;
            self.methods_unwind += unwind as usize;
        }
        self.files_raw += file_raw as usize;
    }

    pub fn print(&self, list_externs: bool) {
        let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
        println!("definitions: {:?}", self.kinds);
        println!("instructions: {}", self.insts);
        let mut ops: Vec<_> = self.ops.iter().collect();
        ops.sort_by(|a, b| b.1.cmp(a.1));
        println!("ops: {}", ops.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "));
        println!(
            "methods: {}  raw-memory (load/store/elem/stackalloc/ptr-conv): {} ({:.1}%)  stackalloc: {} ({:.1}%)",
            self.methods,
            self.methods_raw,
            pct(self.methods_raw, self.methods),
            self.methods_stackalloc,
            pct(self.methods_stackalloc, self.methods)
        );
        println!(
            "methods calling externs: {} ({:.1}%)  with unwind edges: {} ({:.1}%)",
            self.methods_extern_call,
            pct(self.methods_extern_call, self.methods),
            self.methods_unwind,
            pct(self.methods_unwind, self.methods)
        );
        println!(
            "files with raw-memory methods: {} of {} ({:.1}%)",
            self.files_raw,
            self.files,
            pct(self.files_raw, self.files)
        );
        let sn = self.externs.keys().filter(|k| k.starts_with("scalanative_")).count();
        println!(
            "extern decls: {}  distinct extern symbols called: {} (scalanative_*: {sn})",
            self.extern_decls,
            self.externs.len()
        );
        if list_externs {
            for (name, n) in &self.externs {
                println!("  extern {name} x{n}");
            }
        }
    }
}

fn extern_symbol(pool: &Pool, v: ValId) -> Option<String> {
    let Val::Global(g, _) = pool.val(v) else { return None };
    demangle::extern_name(pool.member_sig(*g)?)
}

fn kind_name(k: &DefnKind) -> &'static str {
    match k {
        DefnKind::Var { .. } => "var",
        DefnKind::Const { .. } => "const",
        DefnKind::Declare { .. } => "declare",
        DefnKind::Define { .. } => "define",
        DefnKind::Trait { .. } => "trait",
        DefnKind::Class { .. } => "class",
        DefnKind::Module { .. } => "module",
    }
}

pub fn op_name(op: &Op) -> &'static str {
    use nir::Conv;
    match op {
        Op::Call { .. } => "call",
        Op::Load { .. } => "load",
        Op::Store { .. } => "store",
        Op::Elem { .. } => "elem",
        Op::Extract { .. } => "extract",
        Op::Insert { .. } => "insert",
        Op::Stackalloc { .. } => "stackalloc",
        Op::Bin { .. } => "bin",
        Op::Comp { .. } => "comp",
        Op::Conv { conv: Conv::Ptrtoint | Conv::Inttoptr, .. } => "ptr-conv",
        Op::Conv { .. } => "conv",
        Op::Fence(_) => "fence",
        Op::Classalloc { .. } => "classalloc",
        Op::Fieldload { .. } => "fieldload",
        Op::Fieldstore { .. } => "fieldstore",
        Op::Field { .. } => "field",
        Op::Method { .. } => "method",
        Op::Dynmethod { .. } => "dynmethod",
        Op::Module(_) => "module",
        Op::As { .. } => "as",
        Op::Is { .. } => "is",
        Op::Copy(_) => "copy",
        Op::SizeOf(_) => "sizeof",
        Op::AlignmentOf(_) => "alignmentof",
        Op::Box { .. } => "box",
        Op::Unbox { .. } => "unbox",
        Op::Var(_) => "var",
        Op::Varload(_) => "varload",
        Op::Varstore { .. } => "varstore",
        Op::Arrayalloc { .. } => "arrayalloc",
        Op::Arrayload { .. } => "arrayload",
        Op::Arraystore { .. } => "arraystore",
        Op::Arraylength(_) => "arraylength",
    }
}
