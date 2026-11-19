// Owns the REPL session: starts the module on the library image and answers the page's inputs
// and completion requests. A trap ends the instance; the page then starts anew.

import { load } from "./scivolo.js";

let repl = null;

async function start({ module, image, width, heapMib }) {
  repl = await load(module);
  const failed = repl.start(image, width, heapMib);
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
