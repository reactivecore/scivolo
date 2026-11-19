// The REPL module's ABI (crates/web/src/lib.rs): an argument goes into the buffer `buffer`
// returns; a reply is a little-endian u32 length and UTF-8 bytes.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Instantiates the module from its bytes or a compiled `WebAssembly.Module`. Imports other than
// `env.now_ms` (wasm-bindgen's, of the `zip` crate's `getrandom`) are never called; they throw.
export async function load(wasm) {
  const module = wasm instanceof WebAssembly.Module ? wasm : await WebAssembly.compile(wasm);
  const imports = {};
  for (const { module: from, name } of WebAssembly.Module.imports(module)) {
    imports[from] ??= {};
    imports[from][name] = () => {
      throw new Error(`${from}.${name} is not provided`);
    };
  }
  imports.env.now_ms = () => Date.now();
  return new Repl((await WebAssembly.instantiate(module, imports)).exports);
}

export class Repl {
  constructor(exports) {
    this.x = exports;
  }

  put(bytes) {
    const at = this.x.buffer(bytes.length) >>> 0;
    new Uint8Array(this.x.memory.buffer, at, bytes.length).set(bytes);
  }

  call(bytes, f) {
    this.put(bytes);
    const at = f() >>> 0;
    const len = new DataView(this.x.memory.buffer).getUint32(at, true);
    return decoder.decode(new Uint8Array(this.x.memory.buffer, at + 4, len));
  }

  // Starts the session on the library image's bytes; "" or why it could not start.
  start(image, width, heapMib) {
    return this.call(image, () => this.x.start(width, heapMib));
  }

  eval(text) {
    return this.call(encoder.encode(text), () => this.x.eval()).split("\n").slice(0, -1);
  }

  complete(text, cursor) {
    const at = encoder.encode(text.slice(0, cursor)).length;
    const reply = this.call(encoder.encode(text), () => this.x.complete(at));
    return reply === "" ? [] : reply.split("\n").map((line) => {
      const [header, ...labels] = line.split("\t");
      return { header, labels };
    });
  }

  highlight(text) {
    return this.call(encoder.encode(text), () => this.x.highlight());
  }

  isIncomplete(text) {
    this.put(encoder.encode(text));
    return this.x.is_incomplete() !== 0;
  }

  exitCode() {
    const code = this.x.exit_code();
    return code < 0 ? null : code;
  }

  timeLimit(millis) {
    this.x.time_limit(millis);
  }
}
