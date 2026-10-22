//! Writing the snapshot file of the trained region and the VM's tables by name.

use super::format::{self, Out};
use super::names::{put_global, put_target, put_type};
use super::walk;
use crate::Vm;
use crate::class::Kind;
use crate::heap;

impl Vm {
    /// The snapshot file of what the runtime's initialization built, or why there is none.
    pub fn snapshot_file(&self) -> Result<Vec<u8>, String> {
        let walk = self.snapshot_walk()?;
        let range = self.heap.trained().ok_or("no training")?.range;
        let mut region = vec![0u8; range.len()];
        heap::copy(range.start, region.as_mut_ptr() as usize, range.len());
        let pool = &self.world.pool;
        let mut out = Out::default();
        format::region(&mut out, range.start, &region, self.thread);
        out.u32(walk.targets.len() as u32);
        for (offset, target) in &walk.targets {
            out.u64(*offset as u64);
            put_target(&mut out, target);
        }
        out.u32(self.classes.all.len() as u32);
        for c in &self.classes.all {
            put_global(&mut out, pool, c.name);
            out.u8(match c.kind {
                Kind::Class => 0,
                Kind::Module => 1,
                Kind::Trait => 2,
            });
            out.opt(c.parent);
            out.u32(c.traits.len() as u32);
            c.traits.iter().for_each(|&t| out.u32(t));
            out.u32(c.size);
            out.opt(c.array_elem);
            out.u64(c.rtti as u64);
            out.u32(c.field_order.len() as u32);
            for f in &c.field_order {
                put_global(&mut out, pool, *f);
                out.u32(c.fields[f].offset);
                put_type(&mut out, pool, c.fields[f].ty);
            }
        }
        let addressed = [&self.modules, &self.funcs.globals];
        for table in addressed {
            out.u32(table.len() as u32);
            for (&g, &addr) in table {
                put_global(&mut out, pool, g);
                out.u64(addr as u64);
            }
        }
        out.u32(self.strings.len() as u32);
        for (&s, &addr) in &self.strings {
            out.str(pool.str(s));
            out.u64(addr as u64);
        }
        out.u32(self.linktime.props.len() as u32);
        for (name, &v) in &self.linktime.props {
            out.str(name);
            out.u64(v);
        }
        let rtti = &self.rtti;
        out.opt(rtti.meta);
        out.u64(rtti.meta_size as u64);
        out.u32(rtti.next_class as u32);
        out.u32(rtti.next_trait as u32);
        let gc = &self.gc_classes;
        out.u32(gc.boxed_ptr as u32);
        out.opt(gc.weak);
        out.u32(gc.next_weak as u32);
        out.u32(gc.weak_referent as u32);
        match gc.weak_handler {
            Some(f) => {
                out.u8(1);
                put_target(&mut out, &walk::target(self, f).ok_or("the weak reference handler has no name")?);
            }
            None => out.u8(0),
        }
        Ok(format::file(out))
    }

    /// Writes the snapshot file to `path` through a staging file; a reason on failure.
    pub fn save_snapshot(&self, path: &std::path::Path) -> Result<(), String> {
        let data = self.snapshot_file()?;
        let staging = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&staging, data).map_err(|e| e.to_string())?;
        std::fs::rename(&staging, path).map_err(|e| e.to_string())
    }
}
