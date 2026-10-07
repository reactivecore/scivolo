//! An uncaught exception as dotty's `StackTraceOps` prints it: its header, then its frames up to
//! the initialization of a wrapper object or the value printer and those not shared with the
//! enclosing trace, then its cause and suppressed exceptions likewise.

use crate::line_names::PREFIX;
use vm::{Frame, Thrown};

/// The package of the value printer in the overlay.
const PRINTER: &str = "scalai.repl.";

/// The classes whose cause is reported instead (dotty's `Rendering.rootCause`).
const WRAPPERS: [&str; 4] = [
    "java.lang.ExceptionInInitializerError",
    "java.lang.reflect.InvocationTargetException",
    "java.lang.reflect.UndeclaredThrowableException",
    "java.util.concurrent.ExecutionException",
];

/// The lines printed for `t`.
pub fn lines(t: &Thrown) -> Vec<String> {
    let mut out = Vec::new();
    print(root_cause(t), "", &[], 0, &mut out);
    out
}

fn root_cause(t: &Thrown) -> &Thrown {
    match &t.cause {
        Some(cause) if WRAPPERS.contains(&t.class.as_str()) => root_cause(cause),
        _ => t,
    }
}

/// Appends to `out` the lines of `t`, related to the enclosing throwable by `relation`, without
/// the outermost frames it shares with `share`, indented by `indents` levels.
fn print(t: &Thrown, relation: &str, share: &[Frame], indents: usize, out: &mut Vec<String>) {
    let trace = &t.frames;
    let shared = trace.iter().rev().zip(share.iter().rev()).take_while(|(a, b)| a == b).count();
    let frames = &trace[..trace.len() - shared];
    let shown = frames.iter().take_while(|f| !wrapper_init(f) && !f.class.starts_with(PRINTER)).count();
    let margin = "  ".repeat(indents);
    out.push(format!("{margin}{relation}{}", t.header));
    out.extend(frames[..shown].iter().map(|f| format!("{margin}  at {}", element(f))));
    match (frames.len() - shown, shared) {
        (0, 0) => {}
        (elided, 0) => out.push(format!("{margin}  ... {elided} elided")),
        (0, more) => out.push(format!("{margin}  ... {more} more")),
        (elided, more) => out.push(format!("{margin}  ... {elided} elided and {more} more")),
    }
    if let Some(cause) = &t.cause {
        print(cause, "Caused by: ", trace, indents, out);
    }
    for s in &t.suppressed {
        print(s, "Suppressed: ", frames, indents + 1, out);
    }
}

fn wrapper_init(f: &Frame) -> bool {
    f.class.starts_with(PREFIX) && (f.method == "<init>" || f.method == "<clinit>")
}

fn element(f: &Frame) -> String {
    let file = f.file.as_deref().unwrap_or("null");
    format!("{}.{}({file}:{})", f.class, f.method, f.line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(class: &str, method: &str, file: Option<&str>, line: i32) -> Frame {
        Frame { class: class.into(), method: method.into(), file: file.map(Into::into), line }
    }

    fn thrown(class: &str, frames: Vec<Frame>) -> Thrown {
        let (description, class, header) = (class.to_owned(), class.to_owned(), class.to_owned());
        Thrown { description, class, header, frames, cause: None, suppressed: Vec::new() }
    }

    fn init() -> Frame {
        frame("rs$line$3$", "<init>", Some("rs$line$3"), 1)
    }

    #[test]
    fn cuts_at_the_wrapper_initialization() {
        let t = thrown(
            "E",
            vec![
                frame("rs$line$2$", "f", Some("rs$line$2"), 1),
                frame("Lib", "g", None, -1),
                init(),
                frame("scala.Run", "main", Some("Run.scala"), 0),
            ],
        );
        assert_eq!(
            lines(&t),
            ["E", "  at rs$line$2$.f(rs$line$2:1)", "  at Lib.g(null:-1)", "  ... 2 elided"]
        );
    }

    #[test]
    fn cuts_at_the_value_printer() {
        let t = thrown(
            "E",
            vec![
                frame("rs$line$2$C", "toString", Some("rs$line$2"), 1),
                frame("scalai.repl.Renderer", "rec", Some("Renderer.scala"), 9),
                frame("scalai.repl.Show$", "apply", Some("Show.scala"), 9),
            ],
        );
        assert_eq!(lines(&t), ["E", "  at rs$line$2$C.toString(rs$line$2:1)", "  ... 2 elided"]);
    }

    #[test]
    fn no_frames_no_elision() {
        assert_eq!(lines(&thrown("E", Vec::new())), ["E"]);
    }

    #[test]
    fn causes_and_suppressed_without_shared_frames() {
        let f = frame("rs$line$2$", "f", Some("rs$line$2"), 1);
        let mut cause = thrown("C", vec![frame("rs$line$1$", "g", Some("rs$line$1"), 1), f.clone(), init()]);
        cause.suppressed.push(thrown("S", vec![f.clone(), init()]));
        let mut t = thrown("E", vec![f, init()]);
        t.cause = Some(Box::new(cause));
        assert_eq!(
            lines(&t),
            [
                "E",
                "  at rs$line$2$.f(rs$line$2:1)",
                "  ... 1 elided",
                "Caused by: C",
                "  at rs$line$1$.g(rs$line$1:1)",
                "  ... 2 more",
                "  Suppressed: S",
                "    at rs$line$2$.f(rs$line$2:1)",
                "    ... 1 elided",
            ]
        );
    }

    #[test]
    fn reports_the_cause_of_a_wrapper() {
        let mut t = thrown("java.util.concurrent.ExecutionException", vec![init()]);
        t.cause = Some(Box::new(thrown("C", vec![init()])));
        assert_eq!(lines(&t), ["C", "  ... 1 elided"]);
    }
}
