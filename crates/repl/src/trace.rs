//! An uncaught exception as the REPL prints it: its description, then its frames up to the
//! initialization of a wrapper object.

use vm::{Frame, Thrown};

/// The lines printed for `t`.
pub fn lines(t: &Thrown) -> Vec<String> {
    let shown = t.frames.iter().take_while(|f| !wrapper_init(f)).count();
    let mut out = vec![t.description.clone()];
    out.extend(t.frames[..shown].iter().map(|f| format!("  at {}", element(f))));
    if shown < t.frames.len() {
        out.push(format!("  ... {} elided", t.frames.len() - shown));
    }
    out
}

fn wrapper_init(f: &Frame) -> bool {
    f.class.starts_with("rs$line$") && (f.method == "<init>" || f.method == "<clinit>")
}

/// `f` as `StackTraceElement.toString` shows it.
fn element(f: &Frame) -> String {
    let at = match (&f.file, f.line) {
        (None, _) => "Unknown Source".to_owned(),
        (Some(file), line) if line <= 0 => file.clone(),
        (Some(file), line) => format!("{file}:{line}"),
    };
    format!("{}.{}({at})", f.class, f.method)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(class: &str, method: &str, file: Option<&str>, line: i32) -> Frame {
        Frame { class: class.into(), method: method.into(), file: file.map(Into::into), line }
    }

    #[test]
    fn cuts_at_the_wrapper_initialization() {
        let t = Thrown {
            description: "java.lang.RuntimeException: boom".into(),
            frames: vec![
                frame("rs$line$2$", "f", Some("rs$line$2"), 1),
                frame("Lib", "g", None, -1),
                frame("rs$line$3$", "<init>", Some("rs$line$3"), 1),
                frame("scala.Run", "main", Some("Run.scala"), 0),
            ],
        };
        assert_eq!(
            lines(&t),
            [
                "java.lang.RuntimeException: boom",
                "  at rs$line$2$.f(rs$line$2:1)",
                "  at Lib.g(Unknown Source)",
                "  ... 2 elided",
            ]
        );
    }

    #[test]
    fn no_frames_no_elision() {
        let t = Thrown { description: "E".into(), frames: Vec::new() };
        assert_eq!(lines(&t), ["E"]);
    }
}
