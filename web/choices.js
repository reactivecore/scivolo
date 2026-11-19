// The completion popup: groups of labels under their headers, one of them selected by the keys.

const list = document.getElementById("choices");

export function hideChoices() {
  list.hidden = true;
  list.replaceChildren();
}

// Shows `groups` ({ header, labels }); a click on a label calls `pick` with it.
export function showChoices(groups, pick) {
  hideChoices();
  for (const { header, labels } of groups) {
    if (header) list.append(Object.assign(document.createElement("li"), { className: "header", textContent: header }));
    for (const label of labels) {
      const li = Object.assign(document.createElement("li"), { role: "option", textContent: label });
      li.onmousedown = (e) => (e.preventDefault(), pick(label));
      list.append(li);
    }
  }
  list.hidden = list.children.length === 0;
}

export function choicesShown() {
  return !list.hidden;
}

// The selected label, if any.
export function selectedChoice() {
  return list.querySelector("[aria-selected=true]")?.textContent ?? null;
}

export function moveChoice(by) {
  const options = [...list.querySelectorAll("[role=option]")];
  const at = options.findIndex((o) => o.ariaSelected === "true");
  options.forEach((o) => (o.ariaSelected = "false"));
  const next = options[(at + by + options.length) % options.length];
  if (next) next.ariaSelected = "true";
}
