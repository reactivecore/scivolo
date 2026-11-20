// `just web-test`: runs the REPL transcripts in the REPL module as `crates/repl/tests/scripted.rs`
// runs them natively, then checks the time limit, refusals, completion, highlighting and exit.
// Usage: node web/test.mjs MODULE.wasm IMAGE.img TRANSCRIPTS_DIR

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { load, runFile } from "./scivolo.js";

// Transcripts that need what the browser refuses: files.
const SKIPPED = ["load.check"];
const PROMPT = "scala> ";
const CONTINUATION = "     |";
// MiB of live objects, small so that garbage must be collected.
const HEAP_MIB = 48;
// Inputs of 256 KiB of garbage each, together more than the cap.
const GARBAGE_INPUTS = 300;

const [wasm, image, dir] = process.argv.slice(2);
const repl = await load(readFileSync(wasm));
const failed = repl.start(readFileSync(image), 80, HEAP_MIB);
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

// Garbage of many inputs and of one long loop is collected under the cap, and values kept across
// the collections, also strings a macro (`f`) built, stay intact.
expect('val keep = Vector.tabulate(20000)(i => s"v$i")', (o) => o[0]?.startsWith("val keep"));
expect('val built = (1 to 100).map(i => f"$i%05d")', (o) => o[0]?.startsWith("val built"));
for (let i = 0; i < GARBAGE_INPUTS; i++) {
  const input = `val n = ${i}; Array.fill(64)(new Array[Byte](4096)).length + f"$n%05d".length`;
  expect(input, (o) => o.at(-1)?.endsWith(": Int = 69"));
}
const lap = "var n = 0L; var i = 0; while i < 4000 do { n += new Array[Byte](100000).length; i += 1 }; n";
expect(lap, (o) => o.at(-1)?.endsWith(": Long = 400000000L"));
expect("keep(12345) + keep.map(_.length).sum + built(41)", (o) => o.at(-1)?.endsWith('= "v1234510889000042"'));
expect("val big = Array.fill(200)(new Array[Byte](1 << 20)); big.length", (o) => o.some((l) => l.includes("java.lang.OutOfMemoryError")));
expect("keep.last", (o) => o.at(-1)?.endsWith('= "v19999"'));

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
if (!/^Welcome to Scivolo \S+ \(target Scala 3\.9\.0\)\./.test(repl.welcome())) throw new Error(`welcome: ${repl.welcome()}`);
if (repl.prompt() !== "> ") throw new Error(`prompt: ${repl.prompt()}`);
expect(":quit", () => true);
if (repl.exitCode() !== 0) throw new Error(`:quit left exit code ${repl.exitCode()}`);

// The file mode: each run in a fresh instance.
const module = await WebAssembly.compile(readFileSync(wasm));
const imageBytes = readFileSync(image);
const options = { width: 80, heapMib: HEAP_MIB };
async function expectRun(text, want) {
  const ran = await runFile(module, imageBytes, options, text);
  if (!want(ran)) throw new Error(`${text}\n=> ${JSON.stringify(ran)}`);
}
await expectRun('@main def m(): Unit =\n  println(f"${41 + 1}%04d")\n  System.err.println("e")\n',
  (r) => r.exit === 0 && r.lines.join("|") === "0042|e");
await expectRun("object Main:\n  def main(args: Array[String]): Unit =\n    println(args.length)\n    sys.exit(3)\n",
  (r) => r.exit === 3 && r.lines.join("|") === "0");
await expectRun('@main def m(): Unit =\n  val x: Int = "a"\n',
  (r) => r.exit === 1 && r.errorLines.join() === "2" && r.lines.at(-1) === "1 error found");
await expectRun("@main def m(): Unit = throw RuntimeException(\"boom\")\n",
  (r) => r.exit === 1 && r.lines[0] === "scivolo: uncaught exception: java.lang.RuntimeException: boom");
await expectRun('def helper(x: Int): Int = {\n  x + 1\n}\n\nprintln("Foo " + helper(3))\n',
  (r) => r.exit === 0 && r.lines.join("|") === "Foo 4");
await expectRun('val a = 1\nprintln(a)\nval b: Int = "x"\n',
  (r) => r.exit === 1 && r.errorLines.join() === "3" && r.lines.some((l) => l.includes('3 |val b: Int = "x"')));
const once = await load(module);
once.loadImage(imageBytes, 80, HEAP_MIB);
once.runFile("@main def m(): Unit = ()\n");
const again = once.runFile("@main def m(): Unit = ()\n");
if (again.exit !== 1 || !again.lines[0]?.includes("an instance runs one file")) {
  throw new Error(`a second run in one instance: ${JSON.stringify(again)}`);
}
console.log(`web-test: ${files.length} transcripts, the browser checks and the file runs pass`);
