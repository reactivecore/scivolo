//! An uncaught exception as dotty's `StackTraceOps` prints it: its header, then its frames up to
//! the initialization of a wrapper object.

use crate::line_names::PREFIX;
use vm::{Frame, Thrown};

/// The lines printed for `t`.
pub fn lines(t: &Thrown) -> Vec<String> {
    let shown = t.frames.iter().take_while(|f| !wrapper_init(f)).count();
    let mut out = vec![t.header.clone()];
    out.extend(t.frames[..shown].iter().map(|f| format!("  at {}", element(f))));
    if shown < t.frames.len() {
        out.push(format!("  ... {} elided", t.frames.len() - shown));
    }
    out
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

    fn thrown(frames: Vec<Frame>) -> Thrown {
        let description = "E".to_owned();
        Thrown { header: description.clone(), description, frames }
    }

    #[test]
    fn cuts_at_the_wrapper_initialization() {
        let t = thrown(vec![
            frame("rs$line$2$", "f", Some("rs$line$2"), 1),
            frame("Lib", "g", None, -1),
            frame("rs$line$3$", "<init>", Some("rs$line$3"), 1),
            frame("scala.Run", "main", Some("Run.scala"), 0),
        ]);
        assert_eq!(
            lines(&t),
            ["E", "  at rs$line$2$.f(rs$line$2:1)", "  at Lib.g(null:-1)", "  ... 2 elided"]
        );
    }

    #[test]
    fn no_frames_no_elision() {
        assert_eq!(lines(&thrown(Vec::new())), ["E"]);
    }
}
