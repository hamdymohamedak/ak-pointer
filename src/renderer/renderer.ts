import { type Action, type InputEvent, formatAction, sanitizeActionName } from "../action.ts";
import { applyTextRun, describeEvent, syncHeader, textRuns } from "../edit.ts";

interface AppChoice {
  owner: string;
  title: string;
  w: number;
  h: number;
  visible: boolean;
}

interface CatalogEntry {
  name: string;
  summary: string;
  window: string;
  readable: boolean;
}

interface Draft {
  start: number;
  end: number;
  original: string;
  edited: string;
}

interface EditorState {
  name: string;
  path: string;
  action: Action;
  owner: string;
  title: string;
  drafts: Draft[];
  showMoves: boolean;
  rawMode: boolean;
  raw: string;
  message: string;
}

interface AkApi {
  listApps(): Promise<{ apps: AppChoice[]; keyboard: boolean }>;
  catalog(): Promise<CatalogEntry[]>;
  startRecording(payload: {
    name: string;
    apps: Array<{ owner: string; title: string }>;
    replace: boolean;
  }): Promise<{
    ok: boolean;
    message: string;
    busy: boolean;
    started?: boolean;
    needReplace?: boolean;
    sanitized?: string;
  }>;
  stop(): Promise<{ message: string; busy: boolean }>;
  poll(): Promise<{ busy: boolean; message: string; catalog?: CatalogEntry[] }>;
  play(names: string[]): Promise<{ ok: boolean; message: string; busy: boolean }>;
  load(name: string): Promise<{ ok: boolean; message?: string; name?: string; path?: string; action?: Action }>;
  save(name: string, action: Action): Promise<{ ok: boolean; message: string; catalog?: CatalogEntry[] }>;
  saveRaw(name: string, text: string): Promise<{
    ok: boolean;
    message: string;
    action?: Action;
    catalog?: CatalogEntry[];
  }>;
  delete(name: string): Promise<{ ok: boolean; message: string; catalog?: CatalogEntry[] }>;
}

const ak = (window as unknown as { ak?: AkApi }).ak;

const state: {
  apps: AppChoice[];
  picked: Array<{ owner: string; title: string }>;
  catalog: CatalogEntry[];
  queue: string[];
  message: string;
  busy: boolean;
  keyboard: boolean;
  replaceName: string;
  confirmDelete: string;
  editor: EditorState | null;
} = {
  apps: [],
  picked: [],
  catalog: [],
  queue: [],
  message: "Choose the app, record an action, then play the list in order.",
  busy: false,
  keyboard: true,
  replaceName: "",
  confirmDelete: "",
  editor: null,
};

const appRoot = document.querySelector("#app");
if (!appRoot || !ak) {
  document.body.textContent = "ak-pointer could not start its window bridge.";
  throw new Error("missing bridge");
}
const api = ak;

const header = el("header");
const titleRow = el("div", "row");
titleRow.append(el("h1", "", "ak-pointer"), el("span", "muted", "1 choose the app   ·   2 record   ·   3 play the list"));
const warning = el("div", "warning");
warning.hidden = true;
const controls = el("div", "row");
const nameInput = document.createElement("input");
nameInput.placeholder = "Action name";
nameInput.style.width = "240px";
const recordButton = button("Record", "primary");
const stopButton = button("Stop");
controls.append(nameInput, recordButton, stopButton);
const status = el("div", "status");
header.append(titleRow, warning, controls, status);

const layout = el("div", "layout");
const aside = document.createElement("aside");
const main = document.createElement("main");
layout.append(aside, main);
appRoot.append(header, layout);

recordButton.addEventListener("click", () => void startRecording());
stopButton.addEventListener("click", () => void stop());
nameInput.addEventListener("input", () => {
  if (state.replaceName && sanitizeActionName(nameInput.value) !== state.replaceName) state.replaceName = "";
});

function el(tag: string, className = "", text = ""): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

function button(label: string, className = ""): HTMLButtonElement {
  const node = document.createElement("button");
  node.type = "button";
  node.textContent = label;
  if (className) node.className = className;
  return node;
}

function pickedHas(owner: string, title: string): boolean {
  return state.picked.some((item) => item.owner === owner && item.title === title);
}

function updateChrome(): void {
  status.textContent = state.message;
  warning.hidden = state.keyboard;
  warning.textContent =
    "Keyboard is not tracked yet. System Settings → Privacy & Security → Input Monitoring → enable Electron. If you started ak-pointer from Cursor, enable Cursor too, then run this again.";
  recordButton.disabled = state.busy || state.editor !== null;
  stopButton.disabled = !state.busy;
  nameInput.disabled = state.busy || state.editor !== null;
}

function renderApps(): void {
  aside.replaceChildren();
  aside.append(el("h2", "", "Apps"));
  aside.append(el("p", "muted", "Check every app this recording may touch. A background app is brought to the front."));
  const tools = el("div", "row");
  const refresh = button("Refresh");
  refresh.disabled = state.busy;
  refresh.addEventListener("click", () => void refreshApps(true));
  tools.append(refresh, el("span", "muted", `${state.picked.length} selected`));
  aside.append(tools);
  if (state.apps.length === 0) {
    aside.append(el("p", "muted", "No app windows found."));
    return;
  }
  let lastOwner = "";
  for (const appChoice of state.apps) {
    if (appChoice.owner !== lastOwner) {
      aside.append(el("div", "owner", appChoice.owner));
      lastOwner = appChoice.owner;
    }
    const label = el("label", "choice");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = pickedHas(appChoice.owner, appChoice.title);
    box.disabled = state.busy;
    box.addEventListener("change", () => {
      if (box.checked) state.picked.push({ owner: appChoice.owner, title: appChoice.title });
      else {
        state.picked = state.picked.filter(
          (item) => !(item.owner === appChoice.owner && item.title === appChoice.title),
        );
      }
      renderApps();
    });
    const title = appChoice.title
      ? `${appChoice.title}    ${Math.round(appChoice.w)} × ${Math.round(appChoice.h)}`
      : `${Math.round(appChoice.w)} × ${Math.round(appChoice.h)}`;
    label.append(box, el("span", "", title));
    aside.append(label);
    if (!appChoice.visible) aside.append(el("div", "muted bg", "in the background"));
  }
}

function renderHome(): void {
  main.replaceChildren();
  main.append(el("h2", "", "Play in this order"));
  main.append(el("p", "muted", "Add saved actions below. They run from top to bottom, and each app is brought forward."));
  const tools = el("div", "row");
  const play = button(state.queue.length ? `Play ${state.queue.length} in order` : "Play 0 in order", "primary");
  play.disabled = state.busy || state.queue.length === 0;
  play.addEventListener("click", () => void playNames(state.queue));
  const clear = button("Clear queue");
  clear.disabled = state.busy || state.queue.length === 0;
  clear.addEventListener("click", () => {
    state.queue = [];
    renderHome();
  });
  tools.append(play, clear);
  main.append(tools);
  if (state.queue.length === 0) main.append(el("p", "muted", "No actions in the queue."));
  state.queue.forEach((name, index) => {
    const row = el("div", "row card");
    row.append(el("strong", "", `${index + 1}. ${name}`));
    const up = button("Up");
    up.disabled = state.busy || index === 0;
    up.addEventListener("click", () => moveQueue(index, -1));
    const down = button("Down");
    down.disabled = state.busy || index === state.queue.length - 1;
    down.addEventListener("click", () => moveQueue(index, 1));
    const remove = button("Remove");
    remove.disabled = state.busy;
    remove.addEventListener("click", () => {
      state.queue.splice(index, 1);
      renderHome();
    });
    row.append(up, down, remove);
    main.append(row);
  });

  main.append(el("h2", "", "Saved"));
  if (state.catalog.length === 0) {
    main.append(el("p", "muted", "No saved actions yet."));
    return;
  }
  for (const entry of state.catalog) {
    const card = el("div", "card");
    card.append(el("div", "", `${entry.name}    ${entry.window}`));
    if (entry.summary) card.append(el("div", "muted", entry.summary));
    const row = el("div", "row");
    const edit = button("Edit");
    edit.disabled = state.busy || !entry.readable;
    edit.addEventListener("click", () => void openEditor(entry.name));
    const playOne = button("Play");
    playOne.disabled = state.busy || !entry.readable;
    playOne.addEventListener("click", () => void playNames([entry.name]));
    const queue = button("Add to queue");
    queue.disabled = !entry.readable;
    queue.addEventListener("click", () => {
      state.queue.push(entry.name);
      state.message = `Added ${entry.name} to the queue.`;
      renderHome();
      updateChrome();
    });
    const confirming = state.confirmDelete === entry.name;
    const remove = button(confirming ? "Confirm delete" : "Delete");
    remove.disabled = state.busy;
    remove.addEventListener("click", () => {
      if (!confirming) {
        state.confirmDelete = entry.name;
        renderHome();
        return;
      }
      void deleteAction(entry.name);
    });
    row.append(edit, playOne, queue, remove);
    card.append(row);
    main.append(card);
  }
}

function moveQueue(index: number, delta: number): void {
  const next = index + delta;
  const [item] = state.queue.splice(index, 1);
  state.queue.splice(next, 0, item);
  renderHome();
}

function draftsFrom(action: Action): Draft[] {
  return textRuns(action.events).map((run) => ({
    start: run.start,
    end: run.end,
    original: run.text,
    edited: run.text,
  }));
}

function renderEditor(): void {
  const editor = state.editor;
  if (!editor) return;
  main.replaceChildren();
  const top = el("div", "row");
  const back = button("Back");
  back.addEventListener("click", () => {
    state.editor = null;
    renderHome();
    updateChrome();
  });
  top.append(back, el("h2", "", editor.name), el("span", "muted", editor.path));
  main.append(top);
  main.append(
    el("p", "muted", `Recorded window size ${Math.round(editor.action.windowW)} x ${Math.round(editor.action.windowH)}`),
  );
  const fields = el("div", "row");
  const owner = document.createElement("input");
  owner.value = editor.owner;
  owner.addEventListener("input", () => {
    if (state.editor) state.editor.owner = owner.value;
  });
  const title = document.createElement("input");
  title.value = editor.title;
  title.addEventListener("input", () => {
    if (state.editor) state.editor.title = title.value;
  });
  fields.append(el("span", "", "Window owner"), owner, el("span", "", "Title"), title);
  main.append(fields);
  main.append(el("p", "muted", "Playback looks for this owner and title."));

  const rawLabel = el("label", "choice");
  const rawBox = document.createElement("input");
  rawBox.type = "checkbox";
  rawBox.checked = editor.rawMode;
  rawBox.addEventListener("change", () => {
    if (!state.editor) return;
    state.editor.rawMode = rawBox.checked;
    if (rawBox.checked) {
      state.editor.action = syncHeader(state.editor.action, state.editor.owner, state.editor.title);
      state.editor.raw = formatAction(state.editor.action);
    }
    renderEditor();
  });
  rawLabel.append(rawBox, el("span", "", "Edit the action file as text"));
  main.append(rawLabel);

  if (editor.rawMode) {
    const save = button("Save file", "primary");
    save.addEventListener("click", () => void saveRaw());
    main.append(save);
    const raw = document.createElement("textarea");
    raw.id = "raw";
    raw.value = editor.raw;
    raw.addEventListener("input", () => {
      if (state.editor) state.editor.raw = raw.value;
    });
    main.append(raw);
  } else {
    main.append(el("p", "", "Typed text. A wrong word can be corrected here. Backspaces from the recording are already applied."));
    if (editor.drafts.length === 0) {
      main.append(el("p", "muted", "This action has no plain typing. Delete an event below, or edit the file text."));
    }
    editor.drafts.forEach((draft, index) => {
      main.append(el("div", "", `Typing ${index + 1}`));
      const area = document.createElement("textarea");
      area.rows = 2;
      area.value = draft.edited;
      area.addEventListener("input", () => {
        draft.edited = area.value;
      });
      main.append(area);
    });
    const saveText = button("Save typed text", "primary");
    saveText.disabled = !editor.drafts.some((draft) => draft.edited !== draft.original);
    saveText.addEventListener("click", () => void saveTyped());
    main.append(saveText);
    const meta = el("div", "row");
    meta.append(el("span", "", `${editor.action.events.length} events`));
    const moves = el("label", "choice");
    const movesBox = document.createElement("input");
    movesBox.type = "checkbox";
    movesBox.checked = editor.showMoves;
    movesBox.addEventListener("change", () => {
      if (!state.editor) return;
      state.editor.showMoves = movesBox.checked;
      renderEditor();
    });
    moves.append(movesBox, el("span", "", "Show mouse moves"));
    meta.append(moves);
    main.append(meta);
    editor.action.events.forEach((event, index) => {
      if (!editor.showMoves && event.kind === "move") return;
      const row = el("div", "event");
      row.append(el("code", "", `${String(event.tMs).padStart(6, " ")} ms   ${describeEvent(event)}`));
      const remove = button("Delete");
      remove.addEventListener("click", () => void deleteEvent(index));
      row.append(remove);
      main.append(row);
    });
  }
  if (editor.message) main.append(el("p", "", editor.message));
}

function renderMain(): void {
  if (state.editor) renderEditor();
  else renderHome();
  updateChrome();
}

async function refreshApps(force: boolean): Promise<void> {
  const result = await api.listApps();
  state.keyboard = result.keyboard;
  if (force || JSON.stringify(result.apps) !== JSON.stringify(state.apps)) {
    state.apps = result.apps;
    if (!state.editor) renderApps();
  }
  updateChrome();
}

async function startRecording(): Promise<void> {
  const result = await api.startRecording({
    name: nameInput.value,
    apps: state.picked.map((item) => ({ ...item })),
    replace: state.replaceName.length > 0 && state.replaceName === sanitizeActionName(nameInput.value),
  });
  state.message = result.message;
  state.busy = result.busy;
  if (result.sanitized) nameInput.value = result.sanitized;
  state.replaceName = result.needReplace && result.sanitized ? result.sanitized : "";
  updateChrome();
}

async function stop(): Promise<void> {
  const result = await api.stop();
  state.message = result.message;
  state.busy = true;
  updateChrome();
}

async function playNames(names: string[]): Promise<void> {
  const result = await api.play(names);
  state.message = result.message;
  state.busy = result.busy;
  updateChrome();
}

async function openEditor(name: string): Promise<void> {
  const result = await api.load(name);
  if (!result.ok || !result.action || !result.path) {
    state.message = result.message ?? "Could not open that action.";
    updateChrome();
    return;
  }
  state.editor = {
    name,
    path: result.path,
    action: result.action,
    owner: result.action.windowOwner,
    title: result.action.windowTitle,
    drafts: draftsFrom(result.action),
    showMoves: false,
    rawMode: false,
    raw: formatAction(result.action),
    message: "",
  };
  renderMain();
}

async function saveTyped(): Promise<void> {
  const editor = state.editor;
  if (!editor) return;
  let events: InputEvent[] = editor.action.events.map((event) => ({ ...event }));
  const pending = editor.drafts
    .filter((draft) => draft.edited !== draft.original)
    .map((draft) => ({ start: draft.start, end: draft.end, text: draft.edited }))
    .sort((left, right) => right.start - left.start);
  for (const draft of pending) {
    const applied = applyTextRun(events, draft.start, draft.end, draft.text);
    if (!applied.ok) {
      editor.message = applied.error;
      renderEditor();
      return;
    }
    events = applied.events;
  }
  editor.action = syncHeader({ ...editor.action, events }, editor.owner, editor.title);
  await writeEditor("Saved the typed text.");
}

async function saveRaw(): Promise<void> {
  const editor = state.editor;
  if (!editor) return;
  const result = await api.saveRaw(editor.name, editor.raw);
  state.message = result.message;
  if (result.catalog) state.catalog = result.catalog;
  if (!result.ok || !result.action) {
    editor.message = result.message;
    renderEditor();
    return;
  }
  editor.action = result.action;
  editor.owner = result.action.windowOwner;
  editor.title = result.action.windowTitle;
  editor.drafts = draftsFrom(result.action);
  editor.raw = formatAction(result.action);
  editor.message = "Saved the action file.";
  renderEditor();
}

async function deleteEvent(index: number): Promise<void> {
  const editor = state.editor;
  if (!editor) return;
  if (editor.drafts.some((draft) => draft.edited !== draft.original)) {
    editor.message = "Save the typed text before deleting events.";
    renderEditor();
    return;
  }
  const events = editor.action.events.filter((_, eventIndex) => eventIndex !== index);
  editor.action = syncHeader({ ...editor.action, events }, editor.owner, editor.title);
  await writeEditor("Deleted the event.");
}

async function writeEditor(note: string): Promise<void> {
  const editor = state.editor;
  if (!editor) return;
  editor.action = syncHeader(editor.action, editor.owner, editor.title);
  const result = await api.save(editor.name, editor.action);
  if (result.catalog) state.catalog = result.catalog;
  if (!result.ok) {
    editor.message = result.message;
    renderEditor();
    return;
  }
  editor.drafts = draftsFrom(editor.action);
  editor.raw = formatAction(editor.action);
  editor.message = note;
  state.message = note;
  renderEditor();
}

async function deleteAction(name: string): Promise<void> {
  const result = await api.delete(name);
  state.confirmDelete = "";
  state.message = result.message;
  if (result.catalog) state.catalog = result.catalog;
  state.queue = state.queue.filter((item) => item !== name);
  renderHome();
  updateChrome();
}

async function boot(): Promise<void> {
  state.catalog = await api.catalog();
  await refreshApps(true);
  renderMain();
  window.setInterval(() => {
    void (async () => {
      const result = await api.poll();
      const wasBusy = state.busy;
      state.busy = result.busy;
      state.message = result.message;
      if (result.catalog) state.catalog = result.catalog;
      updateChrome();
      if (wasBusy && !result.busy && !state.editor) renderHome();
    })();
  }, 200);
  window.setInterval(() => {
    if (!state.busy && !state.editor) void refreshApps(false);
  }, 2000);
}

void boot();
