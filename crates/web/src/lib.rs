//! The REPL as a WebAssembly module for the browser (ADR 0127). The page writes its argument
//! into the buffer `buffer` returns; a reply is a little-endian `u32` length and UTF-8 bytes,
//! valid until the next call.
#![cfg(any(target_arch = "wasm32", feature = "portable"))]

mod program;

use libimage::Image;
use program::Setup;
use repl::Session;
use std::cell::RefCell;
use std::time::Duration;

/// How long an input may run before it is stopped, unless the page sets another limit.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// The exit code `exit_code` returns while the session goes on.
const RUNNING: i32 = -1;

/// The module's one session or program setup and its buffers, the only state outside `Vm`
/// here. An instance holds one VM: a session, or a file run (`load_image`, then `run_file` or
/// `run_compiled`).
#[derive(Default)]
struct State {
    session: Option<Session>,
    setup: Option<Setup>,
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
    let reply = match setup(width, heap_mib).and_then(|setup| open(&setup)) {
        Ok(session) => {
            STATE.with_borrow_mut(|s| s.session = Some(session));
            String::new()
        }
        Err(e) => e,
    };
    reply_with(&reply)
}

/// What a VM over the image in the buffer runs with.
fn setup(width: i32, heap_mib: u32) -> Result<Setup, String> {
    let bytes = STATE.with_borrow_mut(|s| std::mem::take(&mut s.buffer)).leak();
    let image: &'static Image = Box::leak(Box::new(Image::from_bytes(bytes).map_err(|e| e.to_string())?));
    Ok(Setup { image, width, cap: (heap_mib as usize) << 20, limit: TIME_LIMIT })
}

/// Opens a session as `setup` says.
fn open(setup: &Setup) -> Result<Session, String> {
    Session::new(program::new_vm(setup)?, setup.image, setup.width).map_err(|e| e.to_string())
}

/// Takes the library image in the buffer for `run_file` or `run_compiled`, with compiler
/// messages `width` columns wide and at most `heap_mib` MiB of live objects; replies with
/// nothing, or with why the image is unusable.
#[unsafe(no_mangle)]
pub extern "C" fn load_image(width: i32, heap_mib: u32) -> *const u8 {
    let reply = match setup(width, heap_mib) {
        Ok(setup) => {
            STATE.with_borrow_mut(|s| s.setup = Some(setup));
            String::new()
        }
        Err(e) => e,
    };
    reply_with(&reply)
}

/// Compiles the `.scala` file in the buffer and runs its main method (`program::run_file`).
#[unsafe(no_mangle)]
pub extern "C" fn run_file() -> *const u8 {
    with_setup(|setup, text| program::run_file(setup, &String::from_utf8_lossy(text)))
}

/// Runs the program compiled by `run_file` of another instance, in the buffer.
#[unsafe(no_mangle)]
pub extern "C" fn run_compiled() -> *const u8 {
    with_setup(program::run_compiled)
}

/// Makes each later input or run stop after `millis` milliseconds of running.
#[unsafe(no_mangle)]
pub extern "C" fn time_limit(millis: u32) {
    let limit = Duration::from_millis(millis.into());
    with_session(|s| s.limit_runs(Some(limit)));
    STATE.with_borrow_mut(|s| s.setup.iter_mut().for_each(|setup| setup.limit = limit));
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

/// Replies with the line the REPL greets with.
#[unsafe(no_mangle)]
pub extern "C" fn welcome() -> *const u8 {
    reply_with(&repl::welcome())
}

/// Replies with the prompt before each input.
#[unsafe(no_mangle)]
pub extern "C" fn prompt() -> *const u8 {
    reply_with(repl::PROMPT)
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

/// Calls `f` with the program setup and the buffer's bytes; replies with what it returns.
fn with_setup(f: impl FnOnce(&Setup, &[u8]) -> Vec<u8>) -> *const u8 {
    let bytes = STATE.with_borrow_mut(|s| std::mem::take(&mut s.buffer));
    let Some(setup) = STATE.with_borrow_mut(|s| s.setup.take()) else {
        return reply_with("R1\nThe library image is not loaded.\n");
    };
    let reply = f(&setup, &bytes);
    STATE.with_borrow_mut(|s| s.setup = Some(setup));
    reply_bytes(&reply)
}

/// Stores `text` as the reply; its address.
fn reply_with(text: &str) -> *const u8 {
    reply_bytes(text.as_bytes())
}

fn reply_bytes(bytes: &[u8]) -> *const u8 {
    STATE.with_borrow_mut(|s| {
        s.reply.clear();
        s.reply.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        s.reply.extend_from_slice(bytes);
        s.reply.as_ptr()
    })
}
