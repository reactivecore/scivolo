// Runs one whole file in a fresh instance of the module and answers with its output; the page
// ends the worker after the answer, which frees the instance's memory, or to stop the run.

import { runFile } from "./scivolo.js";

onmessage = async ({ data: { module, image, width, heapMib, text } }) => {
  try {
    postMessage(await runFile(module, image, { width, heapMib }, text));
  } catch (e) {
    postMessage({ exit: 1, errorLines: [], lines: [`The run failed: ${e}`] });
  }
};
