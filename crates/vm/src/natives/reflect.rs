//! The natives of quote patterns and of `quotes.reflect` (ADR 0029): matches and operations run
//! by the frontend, their answers read back by number.

use super::NativeFn;
use super::quotes::{HOST, QuoteHost, ints, with_host};
use crate::Vm;
use crate::interp::Flow;

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        (HOST, "D12matchPatterniiiEO") => {
            |vm, args| with_host(vm, |h| h.match_pattern(args[1] as u32, args[2] as u32))
        }
        (HOST, "D7matchediiEO") => |vm, args| {
            let thing = |h: &mut dyn QuoteHost| match h.matched(args[1] as u32) {
                Some(Ok(e)) => Ok(e),
                Some(Err(t)) => Ok(!t),
                None => Err("no such match"),
            };
            with_host(vm, thing)
        },
        (HOST, "D7reflectL16java.lang.StringLAi_L16java.lang.StringiEO") => reflect,
        (HOST, "D6resultiiEO") => {
            |vm, args| Ok(vm.quotes.reflected.0.get(args[1] as usize).map_or(0, |&n| n as u32 as u64))
        }
        (HOST, "D10resultTextL16java.lang.StringEO") => |vm, _| match vm.quotes.reflected.1.clone() {
            Some(text) => Ok(vm.new_string(&text)? as u64),
            None => Ok(0),
        },
        (HOST, "D8allocateL16java.lang.StringL16java.lang.ObjectEO") => |vm, args| {
            let name = vm.read_string(args[1] as usize)?;
            let cls = vm.link_top(&name)?;
            Ok(vm.alloc_object(cls) as u64)
        },
        _ => return None,
    };
    Some(f)
}

/// `reflect(op, args, text)`: runs a `quotes.reflect` operation, keeps its answer for `result`
/// and `resultText`, and returns how many numbers it answered.
fn reflect(vm: &mut Vm, args: &[u64]) -> Flow {
    if args[1] == 0 || args[2] == 0 {
        return vm.null_pointer();
    }
    let op = vm.read_string(args[1] as usize)?;
    let numbers = ints(args[2] as usize);
    let text = if args[3] == 0 { String::new() } else { vm.read_string(args[3] as usize)? };
    let mut answer = None;
    let count = with_host(vm, |h| {
        let reflected = h.reflect(&op, &numbers, &text)?;
        let count = reflected.0.len() as u32;
        answer = Some(reflected);
        Ok(count)
    })?;
    vm.quotes.reflected = answer.unwrap_or_default();
    Ok(count)
}
