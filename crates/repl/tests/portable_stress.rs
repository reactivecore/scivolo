//! The REPL transcripts and programs that keep objects across calls, exceptions and inputs, in
//! portable mode with a collection at every safepoint and freed memory filled with garbage: an
//! object the VM holds without rooting it shows as a wrong answer or a crash.
#![cfg(feature = "portable")]

#[allow(dead_code)]
mod transcript;

use repl::Session;
use transcript::{check, run_all, session_with};

/// Inputs that check their own results, each answering `true`.
const PROGRAMS: [&str; 10] = [
    "object Slow { var s = 0; var i = 0; while i < 100 do { s += new Array[Int](10).length; i += 1 } }
     |def two(a: String, b: Int) = a.length + b
     |two(new String(\"abc\") + \"x\", Slow.s) == 1004",
    "object Bad { val x: Int = { var i = 0; while i < 50 do { new Array[Int](10); i += 1 }; throw new RuntimeException(\"bo\" + \"om\") } }
     |(try Bad.x catch case e: RuntimeException => e.getMessage) == \"boom\"",
    "val kept = List.tabulate(200)(i => s\"k$i\"); kept.size == 200",
    "kept.map(_.length).sum == 690 && kept(150) == \"k150\"",
    "def fails(n: Int): Int = if n > 3 then throw new IllegalStateException(s\"n=$n\") else fails(n + 1)
     |(try fails(0) catch case e: IllegalStateException => e.getMessage.length) == 3",
    "var log = Vector.empty[String]
     |def step(i: Int) = try { if i % 3 == 0 then throw new RuntimeException(i.toString) else i.toString } finally log :+= s\"f$i\"
     |(0 until 30).map(i => try step(i) catch case e: RuntimeException => \"e\" + e.getMessage).mkString.length == 60 && log.size == 30",
    "lazy val table = (0 until 500).map(i => i -> (\"v\" * (i % 7))).toMap; table(499) == \"vv\"",
    "val sb = new StringBuilder; for i <- 0 until 300 do sb.append(i).append(','); sb.length == 1090",
    "case class P(a: String, b: List[Int]); val ps = (0 until 100).map(i => P(i.toString, List(i, i))); ps.map(_.b.sum).sum == 9900 && ps(42).toString == \"P(42,List(42, 42))\"",
    "kept.last + table(6) + ps.last.a == \"k199vvvvvv99\"",
];

fn session() -> Result<Session, String> {
    session_with(&[], vm::Vm::stress_collections)
}

fn run() -> Result<(), String> {
    let mut s = session()?;
    for input in PROGRAMS {
        let text = input.replace("\n     |", "\n");
        let out = s.eval(&text);
        if !out.last().is_some_and(|l| l.ends_with(": Boolean = true")) {
            return Err(format!("{text}\n=> {}", out.join("\n")));
        }
    }
    s.reset();
    run_all(s)
}

#[test]
fn transcripts_under_collection_stress() {
    check(run);
}
