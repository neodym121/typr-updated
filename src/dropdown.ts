// Dropdowns styled like the rest of the app instead of the browser's native
// <select> popup. The <select> stays in the DOM, hidden, as the source of
// truth: code keeps reading .value and listening for "change". After setting
// .value from code, call syncSelect() so the button shows the new choice.

interface Dropdown {
  select: HTMLSelectElement;
  root: HTMLElement;
  trigger: HTMLButtonElement;
  label: HTMLElement;
  list: HTMLElement;
  /** Option highlighted by the mouse or the arrow keys */
  active: number;
}

const dropdowns = new Map<HTMLSelectElement, Dropdown>();

export function enhanceSelect(select: HTMLSelectElement) {
  if (dropdowns.has(select)) return;

  const root = document.createElement("div");
  root.className = "dropdown";

  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "dropdown-trigger";
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");

  const label = document.createElement("span");
  label.className = "dropdown-label";
  trigger.append(label);

  const list = document.createElement("div");
  list.className = "dropdown-list";
  list.setAttribute("role", "listbox");
  list.hidden = true;

  select.before(root);
  root.append(select, trigger, list);
  select.classList.add("dropdown-native");
  select.tabIndex = -1;
  select.setAttribute("aria-hidden", "true");

  const dropdown: Dropdown = { select, root, trigger, label, list, active: -1 };
  dropdowns.set(select, dropdown);

  trigger.addEventListener("click", () => (isOpen(dropdown) ? close(dropdown) : open(dropdown)));
  trigger.addEventListener("keydown", (event) => onKeyDown(dropdown, event));
  list.addEventListener("mousedown", (event) => event.preventDefault()); // keep focus on the button

  // Options rebuilt from code (e.g. the microphone list)
  new MutationObserver(() => syncSelect(select)).observe(select, { childList: true, subtree: true });
  syncSelect(select);
}

export function syncSelect(select: HTMLSelectElement) {
  const dropdown = dropdowns.get(select);
  if (!dropdown) return;
  const text = select.selectedOptions[0]?.text ?? "";
  dropdown.label.textContent = text;
  dropdown.trigger.title = text;
  if (isOpen(dropdown)) renderOptions(dropdown);
}

function isOpen(dropdown: Dropdown): boolean {
  return !dropdown.list.hidden;
}

function renderOptions(dropdown: Dropdown) {
  const options = Array.from(dropdown.select.options);
  dropdown.list.replaceChildren(
    ...options.map((option, index) => {
      const item = document.createElement("div");
      item.className = "dropdown-option";
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", String(index === dropdown.select.selectedIndex));
      item.classList.toggle("active", index === dropdown.active);
      item.textContent = option.text;
      item.title = option.text;
      item.addEventListener("mousemove", () => setActive(dropdown, index));
      item.addEventListener("click", () => choose(dropdown, index));
      return item;
    }),
  );
}

function setActive(dropdown: Dropdown, index: number) {
  if (dropdown.active === index) return;
  dropdown.active = index;
  Array.from(dropdown.list.children).forEach((item, i) => item.classList.toggle("active", i === index));
  dropdown.list.children[index]?.scrollIntoView({ block: "nearest" });
}

function open(dropdown: Dropdown) {
  dropdowns.forEach((other) => other !== dropdown && close(other));
  dropdown.active = Math.max(dropdown.select.selectedIndex, 0);
  renderOptions(dropdown);
  dropdown.list.hidden = false;
  dropdown.root.classList.add("open");
  dropdown.trigger.setAttribute("aria-expanded", "true");

  // Open upwards when there's no room below
  const rect = dropdown.trigger.getBoundingClientRect();
  const below = window.innerHeight - rect.bottom;
  dropdown.root.classList.toggle("drop-up", below < dropdown.list.offsetHeight + 12 && rect.top > below);
  dropdown.list.children[dropdown.active]?.scrollIntoView({ block: "nearest" });
}

function close(dropdown: Dropdown) {
  if (!isOpen(dropdown)) return;
  dropdown.list.hidden = true;
  dropdown.root.classList.remove("open", "drop-up");
  dropdown.trigger.setAttribute("aria-expanded", "false");
}

function choose(dropdown: Dropdown, index: number) {
  const { select } = dropdown;
  if (select.selectedIndex !== index) {
    select.selectedIndex = index;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  }
  syncSelect(select);
  close(dropdown);
  dropdown.trigger.focus();
}

function onKeyDown(dropdown: Dropdown, event: KeyboardEvent) {
  const count = dropdown.select.options.length;
  if (count === 0) return;

  if (!isOpen(dropdown)) {
    if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
      event.preventDefault();
      open(dropdown);
    }
    return;
  }

  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      setActive(dropdown, Math.min(dropdown.active + 1, count - 1));
      break;
    case "ArrowUp":
      event.preventDefault();
      setActive(dropdown, Math.max(dropdown.active - 1, 0));
      break;
    case "Home":
      event.preventDefault();
      setActive(dropdown, 0);
      break;
    case "End":
      event.preventDefault();
      setActive(dropdown, count - 1);
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      choose(dropdown, dropdown.active);
      break;
    case "Escape":
      event.preventDefault();
      close(dropdown);
      break;
    case "Tab":
      close(dropdown);
      break;
  }
}

// Clicking anywhere else or leaving the window closes an open list
document.addEventListener("mousedown", (event) => {
  dropdowns.forEach((dropdown) => {
    if (!dropdown.root.contains(event.target as Node)) close(dropdown);
  });
});
window.addEventListener("blur", () => dropdowns.forEach(close));
