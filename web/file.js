// The file mode: an editor for a whole Scala file with a gutter of line numbers and a Run button
// that compiles and runs the file in a worker of its own, its output in a pane below. The mode
// and the file are kept per browser.

import { render } from "./sgr.js";

const KEYS = { mode: "scivolo.mode", text: "scivolo.file" };
const SAMPLE = `@main def hello(): Unit =
  val names = List("Ada", "Grace", "Barbara")
  for (name, i) <- names.zipWithIndex do
    println(f"\${i + 1}%2d. Hello, \$name!")
`;
const INDENT = "  ";
const $ = (id) => document.getElementById(id);
const [code, shade, gutter, out, run, stop, status] =
  ["code", "code-shade", "gutter", "file-out", "run", "file-stop", "file-status"].map($);

let runner = null;
let errorLines = new Set();

function stored(key, fallback) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // The file is kept for this page only.
  }
}

// Whether the page shows the file mode.
export function fileMode() {
  return document.body.dataset.mode === "file";
}

function drawGutter(count) {
  const lines = Array.from({ length: count }, (_, i) => {
    const line = document.createElement("div");
    line.textContent = String(i + 1);
    if (errorLines.has(i + 1)) line.className = "error";
    return line;
  });
  gutter.replaceChildren(...lines);
}

function print(text, kind) {
  const line = document.createElement("div");
  if (kind) line.className = kind;
  line.append(render(text));
  out.append(line);
}

function busy(running) {
  run.disabled = running;
  stop.hidden = !running;
  status.textContent = running ? "Compiling and running…" : "";
}

function finish(reply) {
  runner?.terminate();
  runner = null;
  busy(false);
  out.replaceChildren();
  reply.lines.forEach((l) => print(l));
  if (reply.exit !== null) print(`Exited with code ${reply.exit}.`, "note");
  errorLines = new Set(reply.errorLines);
  drawGutter(code.value.split("\n").length);
}

// Sets up the editor and the mode switch; `highlight` colours Scala source with SGR sequences,
// `toRepl` runs when the REPL mode is chosen. Returns `ready(module, image, options)`, which
// enables Run once the module and the library image are there.
export function setupFileMode(highlight, toRepl) {
  const redraw = () => {
    shade.replaceChildren(render(highlight(code.value)), "\n");
    drawGutter(code.value.split("\n").length);
  };
  const choose = (mode) => {
    document.body.dataset.mode = mode;
    document.querySelectorAll("#modes button").forEach((b) => b.setAttribute("aria-pressed", b.dataset.mode === mode));
    store(KEYS.mode, mode);
    if (mode === "file") code.focus();
    else toRepl();
  };
  document.querySelectorAll("#modes button").forEach((b) => (b.onclick = () => choose(b.dataset.mode)));

  code.value = stored(KEYS.text, SAMPLE);
  code.addEventListener("input", () => {
    errorLines.clear();
    redraw();
    store(KEYS.text, code.value);
  });
  code.addEventListener("keydown", (e) => {
    if (e.key === "Tab" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      document.execCommand("insertText", false, INDENT);
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      run.click();
    }
  });
  redraw();
  choose(stored(KEYS.mode, "repl") === "file" ? "file" : "repl");

  return {
    ready(module, image, options) {
      busy(false);
      run.onclick = () => {
        busy(true);
        out.replaceChildren();
        runner = new Worker(new URL("runner.js", import.meta.url), { type: "module" });
        runner.onmessage = ({ data }) => finish(data);
        runner.onerror = (e) => finish({ exit: 1, errorLines: [], lines: [`The run failed: ${e.message}`] });
        runner.postMessage({ module, image, ...options, text: code.value });
      };
      stop.onclick = () => finish({ exit: null, errorLines: [], lines: ["Stopped."] });
    },
  };
}
