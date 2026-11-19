//! The REPL as a WebAssembly module for the browser (ADR 0127). The page writes its argument
//! into the buffer `buffer` returns; a reply is a little-endian `u32` length and UTF-8 bytes,
//! valid until the next call.
#![cfg(any(target_arch = "wasm32", feature = "portable"))]

use libimage::Image;
use repl::Session;
use std::cell::RefCell;
use std::time::Duration;
use vm::{Classpath, Vm};

/// How long an input may run before it is stopped, unless the page sets another limit.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// The exit code `exit_code` returns while the session goes on.
const RUNNING: i32 = -1;

/// The module's one session and its buffers, the only state outside `Vm` here.
#[derive(Default)]
struct State {
    session: Option<Session>,
    /// The argument of the next call.
    buffer: Vec<u8>,
    /// The reply of the last call.
    reply: Vec<u8>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Returns the address of a buffer of `len` bytes for the argument of the next call.
#[unsafe(no_mangle)]
pub extern "C" fn buffer(len: usize) -> *mut u8 {
    STATE.with_borrow_mut(|s| {
        s.buffer = vec![0; len];
        s.buffer.as_mut_ptr()
    })
}

/// Starts the session on the library image in the buffer, printing values in `width` columns,
/// with at most `heap_mib` MiB of live objects before `OutOfMemoryError`; replies with nothing,
/// or with why it could not start.
#[unsafe(no_mangle)]
pub extern "C" fn start(width: i32, heap_mib: u32) -> *const u8 {
    let bytes = STATE.with_borrow_mut(|s| std::mem::take(&mut s.buffer));
    let reply = match open(bytes.leak(), width, (heap_mib as usize) << 20) {
        Ok(session) => {
            STATE.with_borrow_mut(|s| s.session = Some(session));
            String::new()
        }
        Err(e) => e,
    };
    reply_with(&reply)
}

/// Opens a session over the image `bytes` with a heap cap of `cap` bytes.
fn open(bytes: &'static [u8], width: i32, cap: usize) -> Result<Session, String> {
    let image: &'static Image = Box::leak(Box::new(Image::from_bytes(bytes).map_err(|e| e.to_string())?));
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    vm.set_heap_cap(cap);
    let mut session = Session::new(vm, image, width).map_err(|e| e.to_string())?;
    session.watch_interrupts().map_err(|e| e.to_string())?;
    session.limit_runs(Some(TIME_LIMIT));
    Ok(session)
}

/// Makes each later input stop after `millis` milliseconds of running.
#[unsafe(no_mangle)]
pub extern "C" fn time_limit(millis: u32) {
    with_session(|s| s.limit_runs(Some(Duration::from_millis(millis.into()))));
}

/// Evaluates the input in the buffer; replies with the lines it prints, program output first,
/// each ended by a newline.
#[unsafe(no_mangle)]
pub extern "C" fn eval() -> *const u8 {
    let text = argument();
    let lines = with_session(|s| s.eval(&text)).unwrap_or_else(|| vec![NOT_STARTED.to_owned()]);
    reply_with(&lines.iter().map(|l| format!("{l}\n")).collect::<String>())
}

/// Lists the completions at byte offset `cursor` of the input in the buffer; replies with one
/// line per group: its header, then a tab before each label.
#[unsafe(no_mangle)]
pub extern "C" fn complete(cursor: usize) -> *const u8 {
    let text = argument();
    let groups = with_session(|s| s.complete(&text, cursor.min(text.len()))).unwrap_or_default();
    let lines: Vec<String> = groups
        .iter()
        .map(|g| g.labels.iter().fold(g.header.unwrap_or("").to_owned(), |line, l| line + "\t" + l))
        .collect();
    reply_with(&lines.join("\n"))
}

/// Highlights the input in the buffer; replies with it marked by ANSI SGR sequences.
#[unsafe(no_mangle)]
pub extern "C" fn highlight() -> *const u8 {
    reply_with(&repl::highlight(&argument()))
}

/// Returns 1 if the input in the buffer needs more lines, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn is_incomplete() -> u32 {
    u32::from(repl::is_incomplete(&argument()))
}

/// Returns the code an input ended the session with (`:quit`, `sys.exit`), else `RUNNING`.
#[unsafe(no_mangle)]
pub extern "C" fn exit_code() -> i32 {
    with_session(|s| s.exit()).flatten().unwrap_or(RUNNING)
}

/// The reply to calls that need a session before `start`.
const NOT_STARTED: &str = "The REPL has not started.";

fn argument() -> String {
    STATE.with_borrow_mut(|s| String::from_utf8_lossy(&std::mem::take(&mut s.buffer)).into_owned())
}

/// Runs `f` on the session, taken out of the state so that `f` may call back into the module.
fn with_session<T>(f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let mut session = STATE.with_borrow_mut(|s| s.session.take())?;
    let out = f(&mut session);
    STATE.with_borrow_mut(|s| s.session = Some(session));
    Some(out)
}

/// Stores `text` as the reply; its address.
fn reply_with(text: &str) -> *const u8 {
    STATE.with_borrow_mut(|s| {
        s.reply.clear();
        s.reply.extend_from_slice(&(text.len() as u32).to_le_bytes());
        s.reply.extend_from_slice(text.as_bytes());
        s.reply.as_ptr()
    })
}
