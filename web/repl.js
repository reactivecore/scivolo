// The page: a transcript and an input line. The session runs in a worker; a second instance of
// the module without the library image highlights the input and decides when it is complete.

import { choicesShown, hideChoices, moveChoice, selectedChoice, showChoices, wordStart } from "./choices.js";
import { download } from "./download.js";
import { fileMode, setupFileMode } from "./file.js";
import { recall, remember } from "./history.js";
import { highlight, light, startLight } from "./light.js";
import { render } from "./sgr.js";

const MODULE = new URL("scivolo.wasm", import.meta.url);
const IMAGE = new URL("libs.img.gz", import.meta.url);
const WIDTH = 80;
// MiB of live objects before OutOfMemoryError.
const HEAP_MIB = 512;
const $ = (id) => document.getElementById(id);
const [out, input, shade, status, stop, prompt] = ["out", "in", "shade", "status", "stop", "prompt"].map($);

let module = null;
let image = null;
let ready = false;
let worker = null;
let pending = null;
let nextId = 0;

function print(text, kind) {
  const line = document.createElement("div");
  if (kind) line.className = kind;
  line.append(render(text));
  out.append(line);
  line.scrollIntoView({ block: "nearest" });
}

function redraw() {
  const text = input.value;
  shade.replaceChildren(render(highlight(text)), "\n");
  input.rows = text.split("\n").length;
}

function ask(message) {
  return new Promise((resolve) => {
    pending = { id: ++nextId, resolve };
    worker.postMessage({ ...message, id: pending.id });
  });
}

function busy(running) {
  input.disabled = running;
  stop.hidden = !running;
  if (!running) input.focus();
}

function onMessage({ data }) {
  if (data.type === "ready") {
    ready = true;
    status.textContent = "";
    busy(false);
  } else if (data.type === "error" || (data.type === "crashed" && !ready)) {
    status.textContent = `The REPL could not start: ${data.message}`;
  } else if (data.type === "crashed") {
    print(`The session ended (${data.message}); a new one starts.`, "note");
    restart();
  } else if (pending && data.id === pending.id) {
    const { resolve } = pending;
    pending = null;
    resolve(data);
  }
}

function restart() {
  worker?.terminate();
  pending?.resolve(null);
  pending = null;
  busy(true);
  stop.hidden = true;
  ready = false;
  status.textContent = "Starting the session…";
  worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
  worker.onmessage = onMessage;
  worker.onerror = (e) => onMessage({ data: { type: "crashed", message: e.message } });
  worker.postMessage({ type: "start", module, image, width: WIDTH, heapMib: HEAP_MIB });
}

// Echoes the input highlighted, its first line after the prompt.
function echo(text) {
  const indent = " ".repeat(prompt.textContent.length);
  const lit = highlight(text).replaceAll("\n", `\n${indent}`);
  const line = document.createElement("div");
  const head = document.createElement("span");
  head.className = "prompt";
  head.textContent = prompt.textContent;
  line.append(head, render(lit));
  out.append(line);
  line.scrollIntoView({ block: "nearest" });
}

// Runs the input; its output and a blank line follow it, as in the native REPL.
async function run(text) {
  echo(text);
  input.value = "";
  redraw();
  if (text.trim() === "") return print("");
  remember(text);
  busy(true);
  const reply = await ask({ type: "eval", text });
  reply?.lines.forEach((l) => print(l));
  const exit = reply?.exit ?? null;
  if (exit !== null) print(`The session ended with exit code ${exit}; a new one starts.`, "note");
  print("");
  if (exit !== null) restart();
  else if (reply) busy(false);
}

function insert(label) {
  const [text, cursor] = [input.value, input.selectionStart];
  const start = wordStart(text, cursor);
  input.value = text.slice(0, start) + label + text.slice(cursor);
  input.selectionStart = input.selectionEnd = start + label.length;
  hideChoices();
  redraw();
}

async function complete() {
  const [text, cursor] = [input.value, input.selectionStart];
  const reply = await ask({ type: "complete", text, cursor });
  const labels = reply?.groups.flatMap((g) => g.labels) ?? [];
  if (labels.length === 0) return;
  const common = labels.reduce((a, b) => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return a.slice(0, i); });
  if (labels.length === 1 || common.length > cursor - wordStart(text, cursor)) insert(common);
  if (labels.length > 1) showChoices(reply.groups, insert);
}

function walk(by) {
  const text = recall(by);
  if (text === null) return;
  input.value = text;
  redraw();
}

input.addEventListener("input", () => (hideChoices(), redraw()));
input.addEventListener("keydown", (e) => {
  const selected = selectedChoice();
  if (choicesShown() && ["ArrowDown", "ArrowUp"].includes(e.key)) {
    e.preventDefault();
    moveChoice(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Escape") hideChoices();
  else if (e.key === "Enter" && selected !== null) {
    e.preventDefault();
    insert(selected);
  } else if (e.key === "Enter" && !e.shiftKey) {
    const text = input.value;
    if (light((r) => r.isIncomplete(text), false)) return;
    e.preventDefault();
    run(text);
  } else if (e.key === "Tab" && !e.shiftKey) {
    e.preventDefault();
    complete();
  } else if (e.key === "ArrowUp" && !input.value.slice(0, input.selectionStart).includes("\n")) {
    e.preventDefault();
    walk(1);
  } else if (e.key === "ArrowDown" && !input.value.slice(input.selectionEnd).includes("\n")) {
    e.preventDefault();
    walk(-1);
  }
});
document.addEventListener("mouseup", () => {
  if (!fileMode() && document.getSelection().isCollapsed) input.focus();
});
document.addEventListener("keydown", (e) => {
  if (!fileMode() && document.activeElement !== input && !e.ctrlKey && !e.metaKey && !e.altKey) input.focus();
});
stop.onclick = () => {
  print("Stopped; the session is reset.", "note");
  restart();
};

const files = setupFileMode(() => input.focus());
const note = (text) => (status.textContent = text, files.note(text));
try {
  module = await WebAssembly.compile(await (await fetch(MODULE)).arrayBuffer());
  const editor = await startLight(module);
  files.redraw();
  prompt.textContent = editor.prompt();
  print(editor.welcome());
  print("");
  const mb = (n) => (n / 1e6).toFixed(1);
  image = await download(IMAGE, (loaded, total) => {
    note(`Downloading the Scala libraries: ${mb(loaded)}${total ? ` of ${mb(total)}` : ""} MB`);
  });
  files.ready(module, image, { width: WIDTH, heapMib: HEAP_MIB });
  restart();
} catch (e) {
  if (!window.reloadFresh()) note(`The page could not load: ${e}`);
}
