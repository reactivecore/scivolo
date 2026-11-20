// The REPL inputs run so far, walked from the newest with ↑ and back with ↓.

const entries = [];
let back = 0;

// Adds `text` unless it repeats the newest input, and starts the walk anew.
export function remember(text) {
  if (entries.at(-1) !== text) entries.push(text);
  back = 0;
}

// Moves the walk `by` inputs to older ones (negative: newer); returns the input reached, "" past
// the newest, or null at either end.
export function recall(by) {
  const at = Math.min(Math.max(back + by, 0), entries.length);
  if (at === back) return null;
  back = at;
  return back === 0 ? "" : entries[entries.length - back];
}
