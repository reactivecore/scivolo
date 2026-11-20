//! The file mode: a whole `.scala` file compiled and run in the module's one VM. A reply starts
//! with `R` and the run, or with `C` and the program compiled for a fresh instance, after a
//! compile that ran a macro.

use libimage::Image;
use lower::NirFile;
use repl::{Program, Ran};
use std::time::Duration;
use vm::{Classpath, Vm};

/// The name of the file a program is compiled from.
const FILE: &str = "main.scala";

/// What a VM of this instance runs with: the library image, the columns of compiler messages,
/// the heap cap in bytes and the time limit of a run.
pub struct Setup {
    pub image: &'static Image,
    pub width: i32,
    pub cap: usize,
    pub limit: Duration,
}

/// Creates the VM over `setup`'s image, which watches interrupts and stops runs at its limit.
pub fn new_vm(setup: &Setup) -> Result<Vm, String> {
    let classpath = Classpath::with_image(&[], setup.image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    vm.set_heap_cap(setup.cap);
    vm.watch_interrupts().map_err(|e| e.to_string())?;
    vm.limit_runs(Some(setup.limit));
    Ok(vm)
}

/// Compiles `text` and runs it in this instance's VM unless a macro ran in that VM; the reply.
pub fn run_file(setup: &Setup, text: &str) -> Vec<u8> {
    let mut vm = match new_vm(setup) {
        Ok(vm) => vm,
        Err(e) => return failed(e),
    };
    match repl::compile_program((FILE, text), setup.image, setup.width as usize, &mut vm) {
        Err(ran) => ran_reply(ran),
        Ok(program) if vm.macro_expansions() > 0 => encode(&program),
        Ok(program) => ran_reply(repl::run_program(&mut vm, program)),
    }
}

/// Runs the program `run_file` replied with in this instance's VM; the reply.
pub fn run_compiled(setup: &Setup, bytes: &[u8]) -> Vec<u8> {
    let Some(program) = decode(bytes) else {
        return failed("the compiled program is damaged".to_owned());
    };
    match new_vm(setup) {
        Ok(mut vm) => ran_reply(repl::run_program(&mut vm, program)),
        Err(e) => failed(e),
    }
}

fn failed(why: String) -> Vec<u8> {
    ran_reply(Ran { lines: vec![why], exit: 1, error_lines: Vec::new() })
}

/// `R`, the exit code and a tab before each line with a compile error, then the lines printed,
/// each ended by a newline.
fn ran_reply(ran: Ran) -> Vec<u8> {
    let mut out = format!("R{}", ran.exit);
    ran.error_lines.iter().for_each(|l| out += &format!("\t{l}"));
    out.push('\n');
    ran.lines.iter().for_each(|l| out += &format!("{l}\n"));
    out.into_bytes()
}

/// `C`, then the main class, the warnings and the files, each part a little-endian `u32`
/// length and its bytes, each list its length and its parts.
fn encode(program: &Program) -> Vec<u8> {
    let mut out = vec![b'C'];
    let put = |out: &mut Vec<u8>, part: &[u8]| {
        out.extend_from_slice(&(part.len() as u32).to_le_bytes());
        out.extend_from_slice(part);
    };
    put(&mut out, program.main.as_bytes());
    out.extend_from_slice(&(program.warnings.len() as u32).to_le_bytes());
    program.warnings.iter().for_each(|w| put(&mut out, w.as_bytes()));
    out.extend_from_slice(&(program.files.len() as u32).to_le_bytes());
    for f in &program.files {
        put(&mut out, f.name.as_bytes());
        put(&mut out, &f.bytes);
    }
    out
}

/// The program `encode` wrote into `bytes`.
fn decode(bytes: &[u8]) -> Option<Program> {
    let rest = &mut bytes.strip_prefix(b"C")?;
    let main = text(rest)?;
    let warnings = (0..count(rest)?).map(|_| text(rest)).collect::<Option<_>>()?;
    let files = (0..count(rest)?).map(|_| Some(NirFile { name: text(rest)?, bytes: part(rest)? }));
    Some(Program { files: files.collect::<Option<_>>()?, main, warnings })
}

/// Takes the little-endian `u32` at the start of `rest`.
fn count(rest: &mut &[u8]) -> Option<usize> {
    let (n, tail) = rest.split_first_chunk::<4>()?;
    *rest = tail;
    Some(u32::from_le_bytes(*n) as usize)
}

/// Takes the length-prefixed part at the start of `rest`.
fn part(rest: &mut &[u8]) -> Option<Vec<u8>> {
    let n = count(rest)?;
    let (part, tail) = rest.split_at_checked(n)?;
    *rest = tail;
    Some(part.to_vec())
}

fn text(rest: &mut &[u8]) -> Option<String> {
    String::from_utf8(part(rest)?).ok()
}

#[cfg(test)]
mod tests {
    use super::{Program, decode, encode};
    use lower::NirFile;

    #[test]
    fn a_compiled_program_is_decoded_as_encoded() {
        let files = vec![NirFile { name: "Main$".into(), bytes: vec![0, 1, 2] }];
        let program = Program { files, main: "Main".into(), warnings: vec!["w".into(), "".into()] };
        let bytes = encode(&program);
        let back = decode(&bytes).unwrap();
        assert_eq!((back.main.as_str(), back.warnings.as_slice()), ("Main", program.warnings.as_slice()));
        assert_eq!((back.files[0].name.as_str(), back.files[0].bytes.as_slice()), ("Main$", &[0, 1, 2][..]));
        assert!(decode(&bytes[..bytes.len() - 1]).is_none());
    }
}
