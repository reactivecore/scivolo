// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nativelib/src/main/scala/scala/scalanative/runtime/package.scala

//! The effects of `runtime.package$.init` that depend on the process, applied to a restored
//! runtime in place of running `init` again.

use crate::interp::Flow;
use crate::layout::ARRAY_HEADER;
use crate::{Vm, VmError, heap};
use nir::Type;

/// The Scala Native release whose `init` `start_restored` ports.
pub const INIT_PORTED_FROM: &str = "0.5.12";

const EXEC_INFO: &str = "scala.scalanative.runtime.ExecInfo$";

/// Where `ExecInfo.filename` and `ExecInfo.startTime` live.
#[derive(Clone, Copy)]
pub struct ExecInfo {
    filename: usize,
    start_time: usize,
}

impl Vm {
    /// The fields of the `ExecInfo` module instance, if it is constructed and has them.
    pub fn exec_info(&self) -> Option<ExecInfo> {
        let pool = &self.world.pool;
        let (&g, &obj) = self.modules.iter().find(|(g, _)| pool.top_name(**g) == EXEC_INFO)?;
        let class = self.class(*self.classes.by_name.get(&g)?);
        let field = |name: &str, long: bool| {
            let prefix = format!("F{}{name}", name.len());
            let g = class
                .field_order
                .iter()
                .find(|f| pool.member_sig(**f).is_some_and(|s| s.starts_with(&prefix)))?;
            let f = &class.fields[g];
            let fits = match pool.ty(f.ty) {
                Type::Long => long,
                Type::Ref { .. } => !long,
                _ => false,
            };
            fits.then_some(obj + f.offset as usize)
        };
        Some(ExecInfo { filename: field("filename", false)?, start_time: field("startTime", true)? })
    }

    /// The program's arguments array, after doing what `init(argc, argv)` does for program
    /// `program` with arguments `args` once the snapshot's `init` has run: the main thread is
    /// set up, `ExecInfo` records the program and the start time.
    pub fn start_restored(&mut self, info: ExecInfo, program: &str, args: &[String]) -> Flow {
        if heap::read::<u64>(self.thread) == 0 {
            return Err(VmError::Unsupported("the snapshot has no main thread".into()).into());
        }
        let cls = self.link_top("scala.scalanative.runtime.ObjectArray")?;
        let array = self.alloc_array(cls, args.len());
        for (i, arg) in args.iter().enumerate() {
            let s = self.new_string(arg)?;
            heap::write(array + ARRAY_HEADER as usize + 8 * i, s);
        }
        let filename = self.new_string(program)?;
        heap::write(info.filename, filename);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        heap::write(info.start_time, now.as_millis() as i64);
        Ok(array as u64)
    }
}
