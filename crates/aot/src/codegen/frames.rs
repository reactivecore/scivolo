//! The code-address table of stack traces (`scivolo_aot_frames`, read by `c/trace.c`): each
//! compiled method's code, class and method name, the source line of each of its instructions,
//! and whether it may build a throwable. Names are UTF-16 in `scivolo_aot_names`, each after its
//! length.

use super::Decls;
use crate::R;
use crate::blob::Blob;
use crate::reach::Reach;
use cranelift_codegen::Context;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{GlobalId, PosId};
use rustc_hash::FxHashMap;

/// The size of an entry of `scivolo_aot_frames` and of one of its lines.
const ENTRY: usize = 56;
const LINE: usize = 16;

struct Compiled {
    func: FuncId,
    method: GlobalId,
    size: u32,
    /// Code ranges and the position of the instruction each was compiled from.
    positions: Vec<(u32, u32, PosId)>,
}

#[derive(Default)]
pub struct Frames(Vec<Compiled>);

#[derive(Default)]
struct Names {
    units: Vec<u16>,
    at: FxHashMap<String, u64>,
}

impl Names {
    /// The offset of `s` in the names, in UTF-16 units.
    fn of(&mut self, s: &str) -> u64 {
        if let Some(&at) = self.at.get(s) {
            return at;
        }
        let at = self.units.len() as u64;
        let units: Vec<u16> = s.encode_utf16().collect();
        self.units.extend([units.len() as u16, (units.len() >> 16) as u16]);
        self.units.extend(units);
        self.at.insert(s.to_owned(), at);
        at
    }
}

impl Frames {
    /// Records method `method`, compiled into `func` by `ctx`.
    pub fn record(&mut self, func: FuncId, method: GlobalId, ctx: &Context) {
        let Some(code) = ctx.compiled_code() else { return };
        let positions = code
            .buffer
            .get_srclocs_sorted()
            .iter()
            .filter_map(|l| {
                let loc = l.loc.as_abs();
                (!loc.is_default()).then(|| (l.start, l.end, PosId(loc.bits())))
            })
            .collect();
        self.0.push(Compiled { func, method, size: code.code_info().total_size, positions });
    }

    /// Defines `scivolo_aot_frames`, `scivolo_aot_lines`, `scivolo_aot_names` and
    /// `scivolo_aot_trace_info` (frame count, `String` and `CharArray` metadata, `String` size,
    /// the name of unknown methods); without entries for executables of single methods.
    pub fn define(mut self, module: &mut ObjectModule, reach: &mut Reach, decls: &mut Decls) -> R<()> {
        if !decls.stack_checks {
            self.0.clear();
        }
        let throwable = linked(reach, "java.lang.Throwable")?;
        let (mut table, mut lines) = (Blob::zeroed(ENTRY * self.0.len().max(1)), Vec::new());
        let mut names = Names::default();
        for (i, c) in self.0.iter().enumerate() {
            let at = ENTRY * i;
            let (class, method) = vm::element_names(&reach.world.pool, c.method);
            let sig = reach.world.pool.member_sig(c.method).unwrap_or("");
            let owner = reach.classes.find(reach.world.owner(c.method));
            let builds = vm::may_build_throwable(sig)
                && owner.is_some_and(|owner| {
                    let ancestors = |a, b| reach.classes.get(a).ancestors.contains(&b);
                    ancestors(owner, throwable) || ancestors(throwable, owner)
                });
            let first = lines.len() / LINE;
            line_runs(reach, c, &mut names, &mut lines);
            table.funcs.push((at, c.func));
            let fields = [c.size as u64, names.of(&class), names.of(&method), first as u64];
            let fields = fields.into_iter().chain([(lines.len() / LINE - first) as u64, builds as u64]);
            for (k, v) in fields.enumerate() {
                put(&mut table, at + 8 + 8 * k, v);
            }
        }
        let export = |module: &mut ObjectModule, name: &str, blob: Blob| {
            let id = module.declare_data(name, Linkage::Export, false, false).map_err(|e| e.to_string())?;
            blob.define(module, id)
        };
        export(module, "scivolo_aot_frames", table)?;
        lines.resize(lines.len().max(LINE), 0);
        export(module, "scivolo_aot_lines", Blob { bytes: lines, ..Blob::default() })?;
        let unknown = names.of("<unknown>");
        let bytes = names.units.iter().flat_map(|u| u.to_le_bytes()).collect();
        export(module, "scivolo_aot_names", Blob { bytes, ..Blob::default() })?;
        let (string, chars) = (linked(reach, "java.lang.String")?, linked(reach, CHAR_ARRAY)?);
        let mut info = Blob::zeroed(40);
        put(&mut info, 0, self.0.len() as u64);
        info.data.push((8, decls.rtti(module, reach, string)?));
        info.data.push((16, decls.rtti(module, reach, chars)?));
        put(&mut info, 24, reach.classes.get(string).size as u64);
        put(&mut info, 32, unknown);
        export(module, "scivolo_aot_trace_info", info)
    }
}

const CHAR_ARRAY: &str = "scala.scalanative.runtime.CharArray";

/// Class `name`, which `tables::link_runtime` linked before the tables were defined.
fn linked(reach: &mut Reach, name: &str) -> R<crate::classes::ClassId> {
    let g = reach.world.top(name);
    reach.classes.find(g).ok_or_else(|| format!("{name} is not linked"))
}

fn put(blob: &mut Blob, at: usize, v: u64) {
    blob.bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// Appends the lines of `c` to `lines`: code range, line and file name of each run of
/// instructions from one line.
fn line_runs(reach: &Reach, c: &Compiled, names: &mut Names, lines: &mut Vec<u8>) {
    let mut runs: Vec<(u32, u32, &str, i32)> = Vec::new();
    for &(start, end, pos) in &c.positions {
        let Some((file, line)) = vm::element_position(&reach.world.pool, pos) else { continue };
        match runs.last_mut() {
            Some(last) if last.1 == start && last.2 == file && last.3 == line => last.1 = end,
            _ => runs.push((start, end, file, line)),
        }
    }
    for (start, end, file, line) in runs {
        let file = names.of(file) as u32;
        lines.extend([start, end, line as u32, file].iter().flat_map(|v| v.to_le_bytes()));
    }
}
