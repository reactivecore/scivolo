// `just web-test`: runs the REPL transcripts in the REPL module as `crates/repl/tests/scripted.rs`
// runs them natively, then checks the time limit, refusals, completion, highlighting and exit.
// Usage: node web/test.mjs MODULE.wasm IMAGE.img TRANSCRIPTS_DIR

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { load } from "./scivolo.js";

// Transcripts that need what the browser refuses: files.
const SKIPPED = ["load.check"];
const PROMPT = "scala> ";
const CONTINUATION = "     |";

const [wasm, image, dir] = process.argv.slice(2);
const repl = await load(readFileSync(wasm));
const failed = repl.start(readFileSync(image), 80);
if (failed !== "") throw new Error(`start: ${failed}`);

const nonBlank = (lines) => lines.map((l) => l.trimEnd()).filter((l) => l !== "");

function answer(expected) {
  const out = [];
  const lines = expected.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (!lines[i].startsWith(PROMPT)) continue;
    const input = [lines[i].slice(PROMPT.length)];
    out.push(lines[i]);
    while (i + 1 < lines.length && lines[i + 1].startsWith(CONTINUATION)) {
      const more = lines[++i].slice(CONTINUATION.length);
      input.push(more.startsWith(" ") ? more.slice(1) : more);
      out.push(lines[i]);
    }
    out.push(...repl.eval(input.join("\n")));
  }
  return out;
}

const differ = [];
const files = readdirSync(dir).filter((f) => f.endsWith(".check") && !SKIPPED.includes(f)).sort();
for (const file of files) {
  const text = readFileSync(join(dir, file), "utf8");
  const actual = nonBlank(answer(text));
  if (actual.join("\n") !== nonBlank(text.split("\n")).join("\n")) differ.push(`${file}:\n${actual.join("\n")}`);
  repl.eval(":reset");
}
if (differ.length > 0) throw new Error(`transcripts differ, actual output:\n\n${differ.join("\n\n")}`);

function expect(text, want) {
  const out = repl.eval(text);
  if (!want(out)) throw new Error(`${text}\n=> ${out.join("\n")}`);
}

const started = Date.now();
repl.timeLimit(1000);
expect("val kept = 41", (o) => o.length === 1);
expect("while true do ()", (o) => o[0]?.startsWith("java.lang.ThreadDeath"));
expect("Thread.sleep(60000)", (o) => o[0]?.startsWith("java.lang.ThreadDeath"));
if (Date.now() - started > 20000) throw new Error("the time limit did not stop the inputs");
expect("kept + 1", (o) => o.length === 1 && o[0].endsWith(": Int = 42"));
expect(":load x.scala", (o) => o[0]?.includes("files are not supported here"));
expect("new Thread(() => ()).start()", (o) => o.some((l) => l.includes("threads are not supported here")));

const groups = repl.complete("List(1).ma", 10);
if (!groups.some((g) => g.labels.includes("map"))) throw new Error(`completion: ${JSON.stringify(groups)}`);
if (!repl.highlight("val x = 1").includes("\x1b[")) throw new Error("highlighting has no colours");
expect(":quit", () => true);
if (repl.exitCode() !== 0) throw new Error(`:quit left exit code ${repl.exitCode()}`);
console.log(`web-test: ${files.length} transcripts and the browser checks pass`);
