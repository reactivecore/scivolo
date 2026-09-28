//! Scala Native's unwinder interface over the interpreter's frames, and the stack trace element
//! for each frame. An "instruction pointer" is `tier << 63 | pc << 32 | method`, or
//! `INLINED | position << 32 | method` for a method inlined into a frame.
//!
//! `get_context` snapshots the frame chain; a cursor is `[generation, index]` into the latest
//! snapshot and ends when a newer snapshot replaced it.

use super::NativeFn;
use crate::class::ClassId;
use crate::interp::Flow;
use crate::stack::Frames;
use crate::{Vm, heap};
use nir::GlobalId;

/// The instruction pointers of the frames at the last `get_context`, innermost first.
#[derive(Default)]
pub struct StackWalk {
    generation: u64,
    ips: Vec<u64>,
}

/// Scala Native reads at most this many frames.
const MAX_FRAMES: usize = 1025;
/// Marks the instruction pointer of a method inlined into a frame: `INLINED | pos << 32 | method`.
const INLINED: u64 = 1 << 62;
const PC_MASK: u64 = 0x3FFF_FFFF;
/// Frames past `MAX_FRAMES` walked for the throwable's constructors skipped at the top.
const SKIPPABLE: usize = 32;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name.strip_prefix("scalanative_")? {
        "unwind_sizeof_context" | "unwind_sizeof_cursor" => |_, _| Ok(16),
        "unw_reg_ip" => |_, _| Ok(0),
        "unwind_get_context" => get_context,
        "unwind_init_local" => |_, args| {
            heap::write(args[0] as usize, heap::read::<u64>(args[1] as usize));
            heap::write(args[0] as usize + 8, 0u64);
            Ok(0)
        },
        "unwind_step" => |vm, args| {
            let (generation, index): (u64, u64) =
                (heap::read(args[0] as usize), heap::read(args[0] as usize + 8));
            let next = index as usize + 1;
            let live = generation == vm.stack_walk.generation && next < vm.stack_walk.ips.len();
            heap::write(args[0] as usize + 8, next as u64);
            Ok(live as u64)
        },
        "unwind_get_reg" => |vm, args| {
            let (generation, index): (u64, u64) =
                (heap::read(args[0] as usize), heap::read(args[0] as usize + 8));
            let walk = &vm.stack_walk;
            match walk.ips.get(index as usize).filter(|_| generation == walk.generation) {
                Some(&ip) => {
                    heap::write(args[2] as usize, ip);
                    Ok(0)
                }
                None => Ok(u32::MAX as u64),
            }
        },
        _ => return None,
    };
    Some(f)
}

/// `get_context(context)`: snapshots the frames from the innermost one, each inlined method as a
/// frame of its own, past the constructors and `fillInStackTrace` of the throwable being built
/// and its supertypes.
fn get_context(vm: &mut Vm, args: &[u64]) -> Flow {
    let mut ips = Vec::new();
    let mut header = vm.frames.innermost();
    while header != 0 && ips.len() < MAX_FRAMES + SKIPPABLE {
        let (method, pc) = (Frames::method(header), Frames::pc(header) as u64 & PC_MASK);
        let (g, tier) = (GlobalId(method as u32), (method >> 32) as u8);
        match vm.inline_chain(g, pc as usize, tier) {
            Some(chain) => ips.extend(
                chain.into_iter().map(|(m, pos)| INLINED | (pos.0 as u64 & PC_MASK) << 32 | m.0 as u64),
            ),
            None => ips.push((tier as u64) << 63 | pc << 32 | g.0 as u64),
        }
        header = Frames::caller(header);
    }
    let throwable = vm.link_top("java.lang.Throwable")?;
    let start = ips.windows(2).take_while(|w| builds_throwable(vm, GlobalId(w[1] as u32), throwable)).count();
    let walk = &mut vm.stack_walk;
    walk.generation += 1;
    walk.ips.clear();
    walk.ips.extend(ips.into_iter().skip(start).take(MAX_FRAMES));
    heap::write(args[0] as usize, walk.generation);
    Ok(0)
}

/// Whether `method` is a constructor or `fillInStackTrace` of a throwable class or a supertype.
fn builds_throwable(vm: &mut Vm, method: GlobalId, throwable: ClassId) -> bool {
    let sig = vm.world.pool.member_sig(method).unwrap_or("");
    if !(sig.starts_with('R') || sig.starts_with("D16fillInStackTrace")) {
        return false;
    }
    let owner = vm.world.owner(method);
    let Ok(class) = vm.link(owner) else { return false };
    vm.is_subclass(class, throwable) || vm.is_subclass(throwable, class)
}

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let is_element = owner == "scala.scalanative.runtime.StackTrace$"
        && sig.starts_with("D20withNameFromUnwind$1jL44scala.scalanative.runtime.StackTrace$Context");
    is_element.then_some(element as NativeFn)
}

/// `withNameFromUnwind(ip, context, position)`: the `StackTraceElement` for `ip`.
fn element(vm: &mut Vm, args: &[u64]) -> Flow {
    let ip = args[1];
    let (method, pc, tier) = (GlobalId(ip as u32), (ip >> 32 & PC_MASK) as usize, (ip >> 63) as u8);
    let (class, name) = match (method.0 as usize) < vm.world.pool.globals.len() {
        true => (
            vm.world.pool.top_name(method).to_owned(),
            method_name(vm.world.pool.member_sig(method).unwrap_or("")),
        ),
        false => ("<unknown>".to_owned(), "<unknown>".to_owned()),
    };
    let position = if ip & INLINED != 0 {
        Some(vm.source_of(nir::PosId(pc as u32)))
    } else {
        vm.source_position(method, pc, tier)
    };
    let (file, line) = match position {
        Some((file, line)) if !file.is_empty() => {
            (Some(file.rsplit('/').next().unwrap_or(file).to_owned()), line as i32 + 1)
        }
        _ => (None, -1),
    };
    let cls = vm.link_top("java.lang.StackTraceElement")?;
    let obj = vm.alloc_object(cls);
    let class = vm.new_string(&class)?;
    let name = vm.new_string(&name)?;
    let file = match file {
        Some(f) => vm.new_string(&f)?,
        None => 0,
    };
    let init =
        vm.world.member(vm.class(cls).name, "RL16java.lang.StringL16java.lang.StringL16java.lang.StringiE");
    vm.call_global(init, &[obj as u64, class as u64, name as u64, file as u64, line as u32 as u64])?;
    Ok(obj as u64)
}

/// The source name of the method with signature `sig`.
fn method_name(sig: &str) -> String {
    if sig.starts_with('R') {
        return "<init>".into();
    }
    let rest = sig.trim_start_matches(['D', 'P', 'C', 'G', 'K']);
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let len: usize = rest[..digits].parse().unwrap_or(0);
    rest.get(digits..digits + len).unwrap_or(sig).to_owned()
}
