// Text with ANSI SGR sequences as spans of classes `b` (bold) and `cNN` (colour NN).

export function render(text) {
  const frag = document.createDocumentFragment();
  let [bold, colour] = [false, null];
  for (const [i, part] of text.split(/\x1b\[([0-9;]*)m/).entries()) {
    if (i % 2 === 1) {
      for (const code of (part || "0").split(";").map(Number)) {
        if (code === 0) [bold, colour] = [false, null];
        else if (code === 1) bold = true;
        else if (code === 22) bold = false;
        else if (code === 39) colour = null;
        else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) colour = code;
      }
    } else if (part !== "") {
      const span = document.createElement("span");
      span.className = [bold ? "b" : "", colour ? `c${colour}` : ""].join(" ").trim();
      span.textContent = part;
      frag.append(span);
    }
  }
  return frag;
}
