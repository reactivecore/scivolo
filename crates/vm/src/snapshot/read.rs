//! Rebuilding the VM's tables from a snapshot file whose region is mapped back in place.

use super::Target;
use super::format::{Image, In, holds};
use super::names::{get_global, get_target, get_type};
use crate::class::{Class, Field, Kind};
use crate::interp::{Unwound, func_ref};
use crate::{Vm, VmError, heap, natives};
use rustc_hash::{FxHashMap, FxHashSet};

impl Vm {
    /// Fills the tables of a VM whose region holds `image`, in place of `bootstrap`.
    pub fn restore(&mut self, image: &mut Image) -> Result<(), VmError> {
        let fail = |e: String| VmError::Unsupported(format!("snapshot: {e}"));
        let region = image.start..image.start + image.region.len();
        let input = &mut image.tables;
        let targets = input.list(|i| Ok((i.u64()? as usize, get_target(i)?))).map_err(fail)?;
        let mut addrs = self.restore_tables(input).map_err(fail)?;
        addrs.extend(targets.iter().map(|(offset, _)| region.start.saturating_add(*offset)));
        if let Some(a) = addrs.into_iter().find(|&a| !holds(&region, a, 8)) {
            return Err(fail(format!("{a:#x} is outside the region")));
        }
        for (offset, target) in targets {
            let value = self.resolve_target(&target)?;
            heap::write(region.start + offset, value);
        }
        self.restored = Some(self.exec_info().ok_or_else(|| fail("no ExecInfo to start".into()))?);
        Ok(())
    }

    /// Drops what a failed `restore` left, for a cold start.
    pub fn forget_snapshot(&mut self) {
        let funcs = std::mem::take(&mut self.funcs);
        (self.funcs.trace, self.funcs.no_inline, self.funcs.profile.on) =
            (funcs.trace, funcs.no_inline, funcs.profile.on);
        self.classes = Default::default();
        self.modules.clear();
        self.strings.clear();
        self.linktime = Default::default();
        self.rtti = Default::default();
        self.gc_classes = Default::default();
        self.callbacks.release();
        self.restored = None;
        self.heap.forget_snapshot();
        self.thread = self.heap.permanent(16, heap::Blob::Words);
    }

    /// Fills the tables; the addresses they hold, unchecked.
    fn restore_tables(&mut self, input: &mut In) -> Result<Vec<usize>, String> {
        let n = input.u32()?;
        for id in 0..n {
            let class = self.read_class(input, id)?;
            self.classes.by_name.insert(class.name, id);
            self.classes.all.push(class);
        }
        let mut addrs: Vec<usize> = self.classes.all.iter().map(|c| c.rtti).collect();
        for globals in [false, true] {
            for (g, addr) in input.list(|i| Ok((get_global(i, &mut self.world)?, i.u64()? as usize)))? {
                addrs.push(addr);
                match globals {
                    false => self.modules.insert(g, addr),
                    true => self.funcs.globals.insert(g, addr),
                };
            }
        }
        for (s, addr) in input.list(|i| Ok((i.str()?.to_owned(), i.u64()? as usize)))? {
            addrs.push(addr);
            let id = nir::StrId(self.world.pool.strings.intern_str(&s));
            self.strings.insert(id, addr);
        }
        self.linktime.props = input.list(|i| Ok((i.str()?.to_owned(), i.u64()?)))?.into_iter().collect();
        self.rtti.meta = input.opt()?;
        self.rtti.meta_size = input.u64()? as usize;
        self.rtti.next_class = input.u32()? as i32;
        self.rtti.next_trait = input.u32()? as i32;
        self.rtti.strings_ready = true;
        let gc = &mut self.gc_classes;
        gc.boxed_ptr = input.u32()? as i32;
        gc.weak = input.opt()?;
        gc.next_weak = input.u32()? as i32;
        gc.weak_referent = input.u32()? as i32;
        let handler = (input.u8()? == 1).then(|| get_target(input)).transpose()?;
        self.update_gc_ids();
        if let Some(handler) = handler {
            let f = self.resolve_target(&handler).map_err(|e| e.to_string())?;
            self.set_weak_handler(f);
        }
        Ok(addrs)
    }

    fn read_class(&mut self, input: &mut In, id: u32) -> Result<Class, String> {
        let name = get_global(input, &mut self.world)?;
        let kind = match input.u8()? {
            0 => Kind::Class,
            1 => Kind::Module,
            2 => Kind::Trait,
            _ => return Err("an unknown class kind".into()),
        };
        let parent = input.opt()?;
        let traits = input.list(In::u32)?;
        let size = input.u32()?;
        let array_elem = input.opt()?;
        let rtti = input.u64()? as usize;
        let fields: Vec<_> = input.list(|i| {
            let f = get_global(i, &mut self.world)?;
            Ok((f, Field { offset: i.u32()?, ty: get_type(i, &mut self.world)? }))
        })?;
        let mut ancestors = FxHashSet::from_iter([id]);
        for &a in parent.iter().chain(&traits) {
            let known = self.classes.all.get(a as usize).ok_or("a class before its ancestors")?;
            ancestors.extend(known.ancestors.iter().copied());
        }
        let field_order = fields.iter().map(|f| f.0).collect();
        let fields: FxHashMap<_, _> = fields.into_iter().collect();
        Ok(Class { name, kind, parent, traits, ancestors, fields, field_order, size, array_elem, rtti })
    }

    /// The value in this process of what `target` names.
    fn resolve_target(&mut self, target: &Target) -> Result<u64, VmError> {
        let func = |vm: &mut Vm, top: &str, sig: &str| {
            let g = vm.world.top(top);
            let g = if sig.is_empty() { g } else { vm.world.member(g, sig) };
            vm.func(g)
        };
        match target {
            Target::Function(top, sig) => Ok(func_ref(func(self, top, sig)?)),
            Target::Trampoline(top, sig) => {
                let f = func(self, top, sig)?;
                self.callable(f).map_err(|u| match u.into_inner() {
                    Unwound::Fatal(e) => e,
                    Unwound::Throw(_) => VmError::Unsupported("snapshot: a trampoline threw".into()),
                })
            }
            Target::Symbol(name) => natives::symbol(name)
                .map(|a| a as u64)
                .ok_or_else(|| VmError::Unsupported(format!("snapshot: no C symbol {name}"))),
        }
    }
}
