// The page: a transcript and an input line. The session runs in a worker; a second instance of
// the module without the library image highlights the input and decides when it is complete.

import { load } from "./scivolo.js";
import { render } from "./sgr.js";

const MODULE = "scivolo.wasm";
const IMAGE = "libs.img.gz";
const WIDTH = 80;
const $ = (id) => document.getElementById(id);
const [out, input, shade, choices, status, stop] = ["out", "in", "shade", "choices", "status", "stop"].map($);

let module = null;
let editor = null;
let worker = null;
let pending = null;
let nextId = 0;
const history = [];
let back = 0;

function print(text, kind) {
  const line = document.createElement("div");
  if (kind) line.className = kind;
  line.append(render(text));
  out.append(line);
  line.scrollIntoView({ block: "nearest" });
}

// Calls `f` on the highlighting instance, made again if it trapped; `fallback` meanwhile.
function light(f, fallback) {
  try {
    return editor ? f(editor) : fallback;
  } catch {
    editor = null;
    load(module).then((r) => (editor = r));
    return fallback;
  }
}

function redraw() {
  const text = input.value;
  shade.replaceChildren(render(light((r) => r.highlight(text), text)), "\n");
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
  if (data.type === "progress") {
    const mb = (n) => (n / 1e6).toFixed(1);
    status.textContent = data.starting ? "Starting the session…"
      : `Downloading the Scala libraries: ${mb(data.loaded)}${data.total ? ` of ${mb(data.total)}` : ""} MB`;
  } else if (data.type === "ready") {
    status.textContent = "";
    busy(false);
  } else if (data.type === "error") {
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
  worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
  worker.onmessage = onMessage;
  worker.onerror = (e) => onMessage({ data: { type: "crashed", message: e.message } });
  worker.postMessage({ type: "start", module, image: new URL(IMAGE, location.href).href, width: WIDTH });
}

async function run(text) {
  if (history.at(-1) !== text) history.push(text);
  back = 0;
  text.split("\n").forEach((l, i) => print(`${i === 0 ? "scala> " : "     | "}${l}`, "input"));
  input.value = "";
  redraw();
  busy(true);
  const reply = await ask({ type: "eval", text });
  if (!reply) return;
  reply.lines.forEach((l) => print(l));
  if (reply.exit !== null) {
    print(`The session ended with exit code ${reply.exit}; a new one starts.`, "note");
    restart();
  } else busy(false);
}

// The start of the identifier before byte `cursor` of `text`.
function wordStart(text, cursor) {
  let i = cursor;
  while (i > 0 && /[\p{L}\p{N}_$]/u.test(text[i - 1])) i--;
  return i;
}

function insert(label) {
  const [text, cursor] = [input.value, input.selectionStart];
  const start = wordStart(text, cursor);
  input.value = text.slice(0, start) + label + text.slice(cursor);
  input.selectionStart = input.selectionEnd = start + label.length;
  hideChoices();
  redraw();
}

function hideChoices() {
  choices.hidden = true;
  choices.replaceChildren();
}

function showChoices(groups) {
  hideChoices();
  for (const { header, labels } of groups) {
    if (header) choices.append(Object.assign(document.createElement("li"), { className: "header", textContent: header }));
    for (const label of labels) {
      const li = Object.assign(document.createElement("li"), { role: "option", textContent: label });
      li.onmousedown = (e) => (e.preventDefault(), insert(label));
      choices.append(li);
    }
  }
  choices.hidden = choices.children.length === 0;
}

function moveChoice(by) {
  const options = [...choices.querySelectorAll("[role=option]")];
  const at = options.findIndex((o) => o.ariaSelected === "true");
  options.forEach((o) => (o.ariaSelected = "false"));
  const next = options[(at + by + options.length) % options.length];
  if (next) next.ariaSelected = "true";
}

async function complete() {
  const [text, cursor] = [input.value, input.selectionStart];
  const reply = await ask({ type: "complete", text, cursor });
  const labels = reply?.groups.flatMap((g) => g.labels) ?? [];
  if (labels.length === 0) return;
  const common = labels.reduce((a, b) => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return a.slice(0, i); });
  if (labels.length === 1 || common.length > cursor - wordStart(text, cursor)) insert(common);
  if (labels.length > 1) showChoices(reply.groups);
}

function recall(by) {
  const at = Math.min(Math.max(back + by, 0), history.length);
  if (at === back) return;
  back = at;
  input.value = back === 0 ? "" : history[history.length - back];
  redraw();
}

input.addEventListener("input", () => (hideChoices(), redraw()));
input.addEventListener("keydown", (e) => {
  const selected = choices.querySelector("[aria-selected=true]");
  if (!choices.hidden && ["ArrowDown", "ArrowUp"].includes(e.key)) {
    e.preventDefault();
    moveChoice(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Escape") hideChoices();
  else if (e.key === "Enter" && selected) {
    e.preventDefault();
    insert(selected.textContent);
  } else if (e.key === "Enter" && !e.shiftKey) {
    const text = input.value;
    if (light((r) => r.isIncomplete(text), false)) return;
    e.preventDefault();
    if (text.trim() !== "") run(text);
  } else if (e.key === "Tab" && !e.shiftKey) {
    e.preventDefault();
    complete();
  } else if (e.key === "ArrowUp" && !input.value.slice(0, input.selectionStart).includes("\n")) {
    e.preventDefault();
    recall(1);
  } else if (e.key === "ArrowDown" && !input.value.slice(input.selectionEnd).includes("\n")) {
    e.preventDefault();
    recall(-1);
  }
});
stop.onclick = () => {
  print("Stopped; the session is reset.", "note");
  restart();
};

module = await WebAssembly.compile(await (await fetch(MODULE)).arrayBuffer());
editor = await load(module);
restart();
