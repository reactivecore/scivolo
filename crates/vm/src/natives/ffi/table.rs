//! The C functions the runtime calls, implemented in Rust for portable mode: memory and string
//! functions, atomics, clocks, `write` to stdout and stderr, `exit` and platform probes. Any
//! other extern is refused as a missing native.

use super::clock;
use crate::interp::Flow;
use crate::natives::NativeFn;
use crate::{VmError, heap};

/// Bytes `scalanative_pthread_*_size` report: room for any platform's pthread object. The
/// pthread functions themselves do nothing, as one thread runs.
const PTHREAD_OBJECT_SIZE: u64 = 64;

/// The `sysconf` name of the online processor count; `sysconf` answers 1, the only processor.
const SC_NPROCESSORS_ONLN: u64 = 1;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    if let Some(rest) = name.strip_prefix("scalanative_atomic_") {
        return atomic(rest);
    }
    let f: NativeFn = match name {
        "memcpy" | "memmove" => |_, a| {
            heap::copy(a[1] as usize, a[0] as usize, a[2] as usize);
            Ok(a[0])
        },
        "memset" => |_, a| {
            (0..a[2] as usize).for_each(|i| heap::write(a[0] as usize + i, a[1] as u8));
            Ok(a[0])
        },
        "memcmp" => |_, a| Ok(memcmp(a[0] as usize, a[1] as usize, a[2] as usize) as u32 as u64),
        "strlen" => |_, a| Ok(strlen(a[0] as usize) as u64),
        "memmem" => |_, a| Ok(memmem(a[0] as usize, a[1] as usize, a[2] as usize, a[3] as usize) as u64),
        "write" => |vm, a| match a[0] as i32 {
            fd @ (1 | 2) => {
                let bytes: Vec<u8> = (0..a[2] as usize).map(|i| heap::read(a[1] as usize + i)).collect();
                vm.record_output(fd, &bytes);
                Ok(a[2])
            }
            _ => Ok(-1i64 as u64),
        },
        "exit" => |_, a| Err(VmError::Exit(a[0] as i32).into()),
        "dlerror" | "scalanative_set_os_props" | "scalanative_stdin_fileno" => |_, _| Ok(0),
        "scalanative_stdout_fileno" => |_, _| Ok(1),
        "scalanative__sc_nprocessors_onln" => |_, _| Ok(SC_NPROCESSORS_ONLN),
        "sysconf" => |_, a| Ok(if a[0] as i32 as u64 == SC_NPROCESSORS_ONLN { 1 } else { -1i64 as u64 }),
        "scalanative_stderr_fileno" => |_, _| Ok(2),
        "scalanative_current_time_millis" => |_, _| Ok(clock::epoch().as_millis() as u64),
        "scalanative_nano_time" => |_, _| Ok(clock::monotonic().as_nanos() as u64),
        _ if name.starts_with("scalanative_pthread_") && name.ends_with("_size") => {
            |_, _| Ok(PTHREAD_OBJECT_SIZE)
        }
        _ if name.starts_with("pthread_") || name.starts_with("scalanative_pthread_") => |_, _| Ok(0),
        _ => return super::heap_externs::extern_fn(name),
    };
    Some(f)
}

fn strlen(s: usize) -> usize {
    (0..).find(|&i| heap::read::<u8>(s + i) == 0).expect("a terminated C string")
}

fn memcmp(a: usize, b: usize, len: usize) -> i32 {
    (0..len)
        .map(|i| heap::read::<u8>(a + i) as i32 - heap::read::<u8>(b + i) as i32)
        .find(|&d| d != 0)
        .unwrap_or(0)
}

fn memmem(hay: usize, hay_len: usize, needle: usize, needle_len: usize) -> usize {
    if needle_len > hay_len {
        return 0;
    }
    (0..=hay_len - needle_len).find(|&i| memcmp(hay + i, needle, needle_len) == 0).map_or(0, |i| hay + i)
}

/// `scalanative_atomic_<op>[_explicit]_<type>`; the memory orders `_explicit` adds are ignored,
/// as one thread runs.
fn atomic(rest: &str) -> Option<NativeFn> {
    if let Some(order) = rest.strip_prefix("memory_order_") {
        return memory_order(order);
    }
    if matches!(rest, "thread_fence" | "signal_fence") {
        return Some(|_, _| Ok(0));
    }
    let (op, ty) = rest.rsplit_once('_')?;
    let op = op.strip_suffix("_explicit").unwrap_or(op);
    let w = width(ty)?;
    Some(match op {
        "init" | "store" => pick(w, [store::<1>, store::<2>, store::<4>, store::<8>]),
        "load" => pick(w, [load::<1>, load::<2>, load::<4>, load::<8>]),
        "compare_exchange_strong" | "compare_exchange_weak" => {
            pick(w, [cas::<1>, cas::<2>, cas::<4>, cas::<8>])
        }
        "exchange" => fetch_by::<XCHG>(w),
        "fetch_add" => fetch_by::<ADD>(w),
        "fetch_sub" => fetch_by::<SUB>(w),
        "fetch_and" => fetch_by::<AND>(w),
        "fetch_or" => fetch_by::<OR>(w),
        "fetch_xor" => fetch_by::<XOR>(w),
        _ => return None,
    })
}

const ADD: u8 = 0;
const SUB: u8 = 1;
const AND: u8 = 2;
const OR: u8 = 3;
const XOR: u8 = 4;
const XCHG: u8 = 5;

/// The one of `fs`, for widths 1, 2, 4 and 8, that serves width `w`.
fn pick(w: usize, fs: [NativeFn; 4]) -> NativeFn {
    fs[w.trailing_zeros() as usize]
}

fn fetch_by<const OP: u8>(w: usize) -> NativeFn {
    pick(w, [fetch::<1, OP>, fetch::<2, OP>, fetch::<4, OP>, fetch::<8, OP>])
}

/// `atomic_fetch_<OP>(ptr, v)` and `atomic_exchange(ptr, v)`: the old value.
fn fetch<const W: usize, const OP: u8>(_: &mut crate::Vm, a: &[u64]) -> Flow {
    let (addr, v) = (a[0] as usize, a[1]);
    let old = read_n::<W>(addr);
    let new = match OP {
        ADD => old.wrapping_add(v),
        SUB => old.wrapping_sub(v),
        AND => old & v,
        OR => old | v,
        XOR => old ^ v,
        _ => v,
    };
    write_n::<W>(addr, new);
    Ok(old)
}

/// `atomic_compare_exchange(ptr, expected, desired)`: whether `*ptr` was `*expected`, which
/// else receives `*ptr`.
fn cas<const W: usize>(_: &mut crate::Vm, a: &[u64]) -> Flow {
    let (addr, expected) = (a[0] as usize, a[1] as usize);
    let current = read_n::<W>(addr);
    if current == read_n::<W>(expected) {
        write_n::<W>(addr, a[2]);
        return Ok(1);
    }
    write_n::<W>(expected, current);
    Ok(0)
}

/// The value of `memory_order_<order>` in C11.
fn memory_order(order: &str) -> Option<NativeFn> {
    Some(match order {
        "relaxed" => |_, _| Ok(0),
        "consume" => |_, _| Ok(1),
        "acquire" => |_, _| Ok(2),
        "release" => |_, _| Ok(3),
        "acq_rel" => |_, _| Ok(4),
        "seq_cst" => |_, _| Ok(5),
        _ => return None,
    })
}

/// The size in bytes of C type `ty` in the VM's 64-bit data model.
fn width(ty: &str) -> Option<usize> {
    Some(match ty {
        "bool" | "char" | "byte" | "ubyte" | "schar" | "uchar" => 1,
        "short" | "ushort" => 2,
        "int" | "uint" => 4,
        "long" | "ulong" | "llong" | "ullong" | "intptr" | "uintptr" | "size" | "ptr" => 8,
        _ => return None,
    })
}

/// Reads `W` bytes at `addr`, zero-extended.
fn read_n<const W: usize>(addr: usize) -> u64 {
    let mut bytes = [0u8; 8];
    (0..W).for_each(|i| bytes[i] = heap::read(addr + i));
    u64::from_le_bytes(bytes)
}

/// Writes the low `W` bytes of `v` at `addr`.
fn write_n<const W: usize>(addr: usize, v: u64) {
    let bytes = v.to_le_bytes();
    (0..W).for_each(|i| heap::write(addr + i, bytes[i]));
}

fn load<const W: usize>(_: &mut crate::Vm, a: &[u64]) -> Flow {
    Ok(read_n::<W>(a[0] as usize))
}

fn store<const W: usize>(_: &mut crate::Vm, a: &[u64]) -> Flow {
    write_n::<W>(a[0] as usize, a[1]);
    Ok(0)
}
