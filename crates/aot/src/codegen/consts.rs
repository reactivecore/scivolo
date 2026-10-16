//! `const` values: data objects holding a value, with relocations for the pointers in it.

use super::Decls;
use crate::R;
use crate::blob::Blob;
use crate::reach::Reach;
use cranelift_module::{DataId, Linkage, Module};
use cranelift_object::ObjectModule;
use nir::{DefnKind, GlobalId, Pool, Val, ValId};
use vm::{align_of, size_of};

/// The size and alignment of value `v`, and the offsets of its members if it is a struct.
fn shape(pool: &Pool, v: ValId) -> R<(usize, usize, Vec<usize>)> {
    Ok(match pool.val(v) {
        Val::True | Val::False | Val::Byte(_) => (1, 1, vec![]),
        Val::Char(_) | Val::Short(_) => (2, 2, vec![]),
        Val::Int(_) | Val::Float(_) => (4, 4, vec![]),
        Val::Zero(ty) => (size_of(pool, *ty) as usize, align_of(pool, *ty) as usize, vec![]),
        Val::ArrayValue(ty, vs) => {
            (size_of(pool, *ty) as usize * vs.len(), align_of(pool, *ty) as usize, vec![])
        }
        Val::ByteString(bytes) => (bytes.len(), 1, vec![]),
        Val::StructValue(vs) => {
            let (mut at, mut max, mut offsets) = (0usize, 1usize, Vec::new());
            for &m in vs {
                let (size, align, _) = shape(pool, m)?;
                at = at.next_multiple_of(align);
                offsets.push(at);
                at += size;
                max = max.max(align);
            }
            (at.next_multiple_of(max), max, offsets)
        }
        _ => (8, 8, vec![]),
    })
}

impl Decls {
    /// The data of global variable or constant `g`: the C variable an extern names, else its
    /// initial value, writable and scanned by the collector for a variable.
    pub fn global_data(&mut self, module: &mut ObjectModule, reach: &mut Reach, g: GlobalId) -> R<DataId> {
        if let Some(&id) = self.globals.get(&g) {
            return Ok(id);
        }
        let defn = reach.world.defn(g).map_err(|e| e.to_string())?;
        let id = match (&defn.kind, super::decls::extern_name(&reach.world.pool, g)) {
            (_, Some(name)) => {
                module.declare_data(&name, Linkage::Import, true, false).map_err(|e| e.to_string())?
            }
            (DefnKind::Var { rhs, .. } | DefnKind::Const { rhs, .. }, None) => {
                let (size, _, _) = shape(&reach.world.pool, *rhs)?;
                let mut blob = Blob::zeroed(size.max(1));
                self.encode(module, reach, *rhs, &mut blob, 0)?;
                let name = format!("_S{}", reach.world.pool.mangle(g));
                let var = matches!(defn.kind, DefnKind::Var { .. });
                let id = module.declare_data(&name, Linkage::Local, var, false).map_err(|e| e.to_string())?;
                blob.define(module, id)?;
                if var {
                    self.roots.push((id, size.max(1)));
                }
                id
            }
            _ => return Err(format!("{} is no variable", reach.world.name(g))),
        };
        self.globals.insert(g, id);
        Ok(id)
    }

    /// `scalai_aot_globals`: the address and size of each global variable, for the collector to
    /// scan, and `scalai_aot_globals_count` of them.
    pub fn define_roots(&mut self, module: &mut ObjectModule) -> R<()> {
        let mut table = Blob::zeroed(16 * self.roots.len().max(1));
        for (i, &(id, size)) in self.roots.iter().enumerate() {
            table.data.push((16 * i, id));
            table.bytes[16 * i + 8..16 * i + 16].copy_from_slice(&(size as u64).to_le_bytes());
        }
        let declare = |module: &mut ObjectModule, name: &str| {
            module.declare_data(name, Linkage::Export, false, false).map_err(|e| e.to_string())
        };
        let id = declare(module, "scalai_aot_globals")?;
        table.define(module, id)?;
        let count = Blob { bytes: (self.roots.len() as u64).to_le_bytes().to_vec(), ..Blob::default() };
        let id = declare(module, "scalai_aot_globals_count")?;
        count.define(module, id)
    }

    /// A data object holding value `v`.
    pub fn constant(&mut self, module: &mut ObjectModule, reach: &mut Reach, v: ValId) -> R<DataId> {
        let (size, _, _) = shape(&reach.world.pool, v)?;
        let mut blob = Blob::zeroed(size.max(1));
        self.encode(module, reach, v, &mut blob, 0)?;
        self.anon += 1;
        let name = format!("scalai_aot_const.{}", self.anon);
        let id = module.declare_data(&name, Linkage::Local, true, false).map_err(|e| e.to_string())?;
        blob.define(module, id)?;
        Ok(id)
    }

    fn encode(
        &mut self,
        module: &mut ObjectModule,
        reach: &mut Reach,
        v: ValId,
        blob: &mut Blob,
        at: usize,
    ) -> R<()> {
        let mut put = |bytes: &[u8]| blob.bytes[at..at + bytes.len()].copy_from_slice(bytes);
        match reach.world.pool.val(v).clone() {
            Val::True => put(&[1]),
            Val::Byte(x) => put(&x.to_le_bytes()),
            Val::Char(x) => put(&x.to_le_bytes()),
            Val::Short(x) => put(&x.to_le_bytes()),
            Val::Int(x) => put(&x.to_le_bytes()),
            Val::Long(x) => put(&x.to_le_bytes()),
            Val::Float(bits) => put(&bits.to_le_bytes()),
            Val::Double(bits) | Val::Size(bits) => put(&bits.to_le_bytes()),
            Val::ByteString(bytes) => put(&bytes),
            Val::False | Val::Null | Val::Zero(_) | Val::Unit => {}
            Val::StructValue(vs) => {
                let (_, _, offsets) = shape(&reach.world.pool, v)?;
                for (m, off) in vs.into_iter().zip(offsets) {
                    self.encode(module, reach, m, blob, at + off)?;
                }
            }
            Val::ArrayValue(ty, vs) => {
                let size = size_of(&reach.world.pool, ty) as usize;
                for (i, m) in vs.into_iter().enumerate() {
                    self.encode(module, reach, m, blob, at + i * size)?;
                }
            }
            Val::String(s) => {
                let pool = &reach.world.pool;
                let units: Vec<u16> = match pool.utf16.get(&s) {
                    Some(units) => units.to_vec(),
                    None => pool.str(s).encode_utf16().collect(),
                };
                blob.data.push((at, self.string(module, reach, &units)?));
            }
            Val::Const(inner) => blob.data.push((at, self.constant(module, reach, inner)?)),
            Val::ClassOf(g) => {
                let cls = reach.link(g)?;
                blob.data.push((at, self.rtti(module, reach, cls)?));
            }
            Val::Global(g, _) => blob.funcs.push((at, self.func(module, reach, g)?)),
            v => return Err(format!("constant {v:?} not supported yet")),
        }
        Ok(())
    }
}
