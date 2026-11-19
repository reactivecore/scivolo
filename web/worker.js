// Owns the REPL session: downloads the library image, starts the module on it and answers the
// page's inputs and completion requests. A trap ends the instance; the page then starts anew.

import { load } from "./scivolo.js";

let repl = null;

async function download(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  const total = Number(response.headers.get("Content-Length")) || 0;
  const reader = response.body.getReader();
  const chunks = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    postMessage({ type: "progress", loaded, total });
  }
  const bytes = new Uint8Array(await new Blob(chunks).arrayBuffer());
  if (bytes[0] !== 0x1f || bytes[1] !== 0x8b) return bytes;
  const unpacked = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(unpacked).arrayBuffer());
}

async function start({ module, image, width }) {
  repl = await load(module);
  const bytes = await download(image);
  postMessage({ type: "progress", loaded: 0, total: 0, starting: true });
  const failed = repl.start(bytes, width);
  postMessage(failed === "" ? { type: "ready" } : { type: "error", message: failed });
}

const handlers = {
  start,
  eval: ({ id, text }) => postMessage({ type: "output", id, lines: repl.eval(text), exit: repl.exitCode() }),
  complete: ({ id, text, cursor }) => postMessage({ type: "completions", id, groups: repl.complete(text, cursor) }),
};

onmessage = async ({ data }) => {
  try {
    await handlers[data.type](data);
  } catch (e) {
    postMessage({ type: "crashed", id: data.id, message: String(e) });
  }
};
