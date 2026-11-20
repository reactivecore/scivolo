// The instance of the module without the library image that both modes use to colour Scala
// source and to decide when a REPL input is complete.

import { load } from "./scivolo.js";

let module = null;
let instance = null;

// Instantiates `compiled`, a `WebAssembly.Module`, as the highlighting instance and returns it.
export async function startLight(compiled) {
  module = compiled;
  instance = await load(module);
  return instance;
}

// Calls `f` on the highlighting instance, made again if it trapped; `fallback` meanwhile.
export function light(f, fallback) {
  try {
    return instance ? f(instance) : fallback;
  } catch {
    instance = null;
    load(module).then((r) => (instance = r));
    return fallback;
  }
}

// Colours `text` with SGR sequences; `text` itself while there is no instance.
export function highlight(text) {
  return light((r) => r.highlight(text), text);
}
