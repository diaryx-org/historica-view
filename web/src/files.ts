// The folder as it stood at one revision, and each file's versions.
//
// The whole view is one moment, `at`, and one selection. The tree is the
// folder at that moment; the timeline is every revision that wrote the
// selection; picking a version moves the moment, and the tree with it.

import { type App, ancestry, href, type Route } from "./app";
import { ago, bytes, element, error, hunksTable, link, short, textTable, title, who } from "./dom";
import type { FileAt, FolderVersion, Hunk, TreeFile, Version } from "./types";

const trees = new Map<string, TreeFile[]>();
const histories = new Map<string, Version[]>();
/// Folders open in the tree, by path. Kept across moments, so travelling in
/// time does not fold the tree back up.
const opened = new Set<string>([""]);
let mode: "content" | "changes" = "content";

function filesAt(app: App, at: string): TreeFile[] {
  let files = trees.get(at);
  if (!files) {
    files = JSON.parse(app.viewer.files(at)) as TreeFile[];
    trees.set(at, files);
  }
  return files;
}

function historyOf(app: App, file: string): Version[] {
  let versions = histories.get(file);
  if (!versions) {
    versions = JSON.parse(app.viewer.fileHistory(file)) as Version[];
    histories.set(file, versions);
  }
  return versions;
}

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const nameOf = (path: string) => path.split("/").pop() || "/";

interface Folder {
  folders: Map<string, Folder>;
  files: TreeFile[];
}

function nest(files: TreeFile[]): Folder {
  const root: Folder = { folders: new Map(), files: [] };
  for (const file of files) {
    let folder = root;
    for (const part of file.path.split("/").slice(0, -1)) {
      let next = folder.folders.get(part);
      if (!next) {
        next = { folders: new Map(), files: [] };
        folder.folders.set(part, next);
      }
      folder = next;
    }
    folder.files.push(file);
  }
  return root;
}

function find(root: Folder, path: string): Folder | undefined {
  let folder: Folder | undefined = root;
  for (const part of path ? path.split("/") : []) folder = folder?.folders.get(part);
  return folder;
}

function timeBar(app: App, route: Route & { view: "files" }): HTMLElement {
  const select = element("select", { "aria-label": "As of revision" });
  for (const revision of app.graph.revisions) {
    const option = element(
      "option",
      { value: revision.id },
      `${short(revision.id)} · ${title(revision.message).slice(0, 60)} · ${ago(revision.epoch)}`,
    );
    if (revision.id === route.at) option.selected = true;
    select.append(option);
  }
  select.addEventListener("change", () => app.go({ ...route, at: select.value }));

  const latest = app.graph.revisions.find((revision) => revision.head)?.id;
  const bar = element("div", { class: "timebar" }, element("span", { class: "quiet" }, "As of"), select);
  if (latest && latest !== route.at) {
    bar.append(
      element("button", { type: "button", class: "latest" }, "Latest"),
    );
    bar.lastElementChild!.addEventListener("click", () => app.go({ ...route, at: latest }));
  }
  return bar;
}

function treeList(app: App, route: Route & { view: "files" }, root: Folder, selected: string | null): HTMLElement {
  const list = element("ul", { class: "tree" });
  const row = (depth: number, label: Node[], classes: string, go: () => void) => {
    const item = element("li", { class: classes, style: `padding-left:${8 + depth * 14}px`, tabindex: "0" }, ...label);
    item.addEventListener("click", go);
    list.append(item);
    return item;
  };
  const walk = (folder: Folder, path: string, depth: number) => {
    for (const [name, child] of [...folder.folders].sort(([a], [b]) => a.localeCompare(b))) {
      const childPath = path ? `${path}/${name}` : name;
      const open = opened.has(childPath);
      row(
        depth,
        [element("span", { class: "caret" }, open ? "▾" : "▸"), element("span", {}, `${name}/`)],
        `folder${!route.file && route.folder === childPath ? " selected" : ""}`,
        () => {
          // A click on the folder already shown folds it; any other opens it.
          if (open && !route.file && route.folder === childPath) opened.delete(childPath);
          else opened.add(childPath);
          app.go({ view: "files", at: route.at, folder: childPath });
        },
      );
      if (open) walk(child, childPath, depth + 1);
    }
    for (const file of folder.files) {
      row(
        depth,
        [element("span", { class: "caret" }, ""), element("span", {}, nameOf(file.path))],
        `file ${file.kind}${file.file === selected ? " selected" : ""}`,
        () => app.go({ view: "files", at: route.at, file: file.file }),
      );
    }
  };
  const top = row(
    0,
    [element("span", { class: "caret" }, "▾"), element("span", {}, "/")],
    `folder${!route.file && !route.folder ? " selected" : ""}`,
    () => app.go({ view: "files", at: route.at, folder: "" }),
  );
  top.classList.add("root");
  walk(root, "", 1);
  return list;
}

function crumbs(app: App, at: string, path: string, isFile: boolean): HTMLElement {
  const nav = element("nav", { class: "crumbs" });
  const parts = path ? path.split("/") : [];
  const root = { view: "files" as const, at, folder: "" };
  nav.append(link("/", href(root), () => app.go(root)));
  parts.forEach((part, index) => {
    const last = index === parts.length - 1;
    if (last && isFile) {
      nav.append(" ", element("strong", {}, part));
      return;
    }
    const folder = { view: "files" as const, at, folder: parts.slice(0, index + 1).join("/") };
    nav.append(" ", link(`${part}/`, href(folder), () => app.go(folder)));
  });
  return nav;
}

/// Revisions that wrote the selection, newest first, with the one in force
/// at `at` marked and the ones `at` does not descend from set apart.
function timeline(
  app: App,
  at: string,
  entries: { revision: string; label: Node; go: Route }[],
  current: string | undefined,
): HTMLElement {
  const before = ancestry(app, at);
  const list = element("ol", { class: "timeline" });
  for (const entry of entries) {
    const revision = app.byId.get(entry.revision);
    if (!revision) continue;
    const classes = [entry.revision === current ? "current" : "", before.has(entry.revision) ? "" : "elsewhere"];
    const item = element(
      "li",
      { class: classes.join(" ").trim(), tabindex: "0", title: before.has(entry.revision) ? "" : "Not behind this moment: later, or on another branch" },
      element("span", { class: "dot" }),
      element("code", { class: "id" }, short(entry.revision)),
      entry.label,
      element("span", { class: "title" }, title(revision.message)),
      element("span", { class: "author" }, who(revision.author)),
      element("time", { datetime: revision.when, title: revision.when }, ago(revision.epoch)),
    );
    item.addEventListener("click", () => app.go(entry.go));
    list.append(item);
  }
  return list;
}

function imageType(path: string): string | null {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  const types: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    avif: "image/avif",
    svg: "image/svg+xml",
  };
  return types[extension] ?? null;
}

function content(app: App, at: string, shown: FileAt): HTMLElement {
  if (shown.kind === "link") return element("p", {}, "A link to ", element("code", {}, shown.target ?? "?"));
  if (shown.kind === "lines") return textTable(shown.text ?? "");
  const size = shown.size === null ? "" : `${bytes(shown.size)}, `;
  if (!shown.held) return element("p", { class: "quiet" }, `${size}held elsewhere: this copy has the history and not the bytes.`);
  const type = imageType(shown.path);
  if (type) {
    const data = app.viewer.fileBytes(at, shown.file);
    if (data) {
      const url = URL.createObjectURL(new Blob([new Uint8Array(data)], { type }));
      return element("figure", { class: "image" }, element("img", { src: url, alt: shown.path }));
    }
  }
  return element("p", { class: "quiet" }, `${size}bytes, not lines.`);
}

function showFile(app: App, route: Route & { view: "files" }, file: string): HTMLElement[] {
  const versions = historyOf(app, file);
  const before = ancestry(app, route.at);
  const index = versions.findIndex((version) => before.has(version.revision));
  const current = index >= 0 ? versions[index] : undefined;
  const shown = JSON.parse(app.viewer.fileAt(route.at, file)) as FileAt | null;
  const path = shown?.path ?? current?.path ?? versions[0]?.path ?? file;

  const heading = element("h2", { class: "path" }, crumbs(app, route.at, path, true));
  const facts = element("p", { class: "quiet" });
  if (shown) {
    facts.append(shown.kind === "lines" ? `${(shown.text ?? "").split("\n").length - 1} lines` : shown.kind);
    if (shown.size !== null) facts.append(` · ${bytes(shown.size)}`);
  } else if (current?.kind === "dropped") {
    facts.append("Dropped at this moment.");
  } else {
    facts.append("Not in the folder at this moment.");
  }
  if (current) {
    const revision = { view: "history" as const, id: current.revision };
    facts.append(" · this version from ", link(short(current.revision), href(revision), () => app.go(revision)));
  }

  const stepTo = (version: Version | undefined, label: string) => {
    const button = element("button", { type: "button" }, label);
    if (!version) button.disabled = true;
    else button.addEventListener("click", () => app.go({ view: "files", at: version.revision, file }));
    return button;
  };
  const older = index >= 0 ? versions[index + 1] : undefined;
  const newer = index > 0 ? versions[index - 1] : index < 0 ? versions[versions.length - 1] : undefined;
  const tabs = element("div", { class: "tabs" });
  for (const choice of ["content", "changes"] as const) {
    const tab = element("button", { type: "button", class: choice === mode ? "on" : "" }, choice === "content" ? "Content" : "Changes");
    tab.addEventListener("click", () => {
      mode = choice;
      app.go(route);
    });
    tabs.append(tab);
  }
  const controls = element("div", { class: "controls" }, stepTo(older, "◀ Older"), stepTo(newer, "Newer ▶"), tabs);

  let body: HTMLElement;
  if (mode === "content") {
    body = shown ? content(app, route.at, shown) : element("p", { class: "quiet" }, "Nothing to show at this moment.");
  } else if (!current) {
    body = element("p", { class: "quiet" }, "The file did not exist yet.");
  } else {
    const hunks = JSON.parse(app.viewer.fileDiff(older?.revision ?? undefined, current.revision, file)) as Hunk[] | null;
    const what = older
      ? element("p", { class: "quiet" }, `What ${short(current.revision)} did, against ${short(older.revision)}.`)
      : element("p", { class: "quiet" }, `The first version, from ${short(current.revision)}.`);
    body = element(
      "div",
      {},
      what,
      hunks === null
        ? element("p", { class: "quiet" }, "Not lines: bytes or a link.")
        : hunks.length === 0
          ? element("p", { class: "quiet" }, current.kind === "moved" ? `Moved to ${current.path}; the content is unchanged.` : "No line changed.")
          : hunksTable(hunks),
    );
  }

  const entries = versions.map((version) => ({
    revision: version.revision,
    label: element(
      "span",
      { class: `kind ${version.kind}`, title: version.path ?? "" },
      version.kind === "moved" ? `moved → ${version.path}` : version.kind,
    ),
    // A dropped file is not in the folder its drop leaves, so its last
    // moment is shown instead.
    go: {
      view: "files" as const,
      at: version.kind === "dropped" ? (app.byId.get(version.revision)?.parents[0] ?? version.revision) : version.revision,
      file,
    },
  }));

  return [
    heading,
    facts,
    element("h3", {}, `${versions.length} version${versions.length === 1 ? "" : "s"}`),
    timeline(app, route.at, entries, current?.revision),
    controls,
    body,
  ];
}

function showFolder(app: App, route: Route & { view: "files" }, root: Folder): HTMLElement[] {
  const path = route.folder ?? "";
  const folder = find(root, path);
  const heading = element("h2", { class: "path" }, crumbs(app, route.at, path, false));
  if (!folder) return [heading, element("p", { class: "quiet" }, "This folder holds nothing at this moment.")];

  const listing = element("ul", { class: "listing" });
  for (const [name] of [...folder.folders].sort(([a], [b]) => a.localeCompare(b))) {
    const target = { view: "files" as const, at: route.at, folder: path ? `${path}/${name}` : name };
    listing.append(element("li", { class: "folder" }, link(`${name}/`, href(target), () => app.go(target))));
  }
  for (const file of folder.files) {
    const target = { view: "files" as const, at: route.at, file: file.file };
    const size = file.size === null ? "" : bytes(file.size);
    listing.append(
      element("li", { class: `file ${file.kind}` }, link(nameOf(file.path), href(target), () => app.go(target)), element("span", { class: "quiet" }, size)),
    );
  }

  const versions = JSON.parse(app.viewer.folderHistory(route.at, path)) as FolderVersion[];
  const before = ancestry(app, route.at);
  const current = versions.find((version) => before.has(version.revision))?.revision;
  const entries = versions.map((version) => ({
    revision: version.revision,
    label: element("span", { class: "kind edited" }, `${version.files} file${version.files === 1 ? "" : "s"}`),
    go: { view: "files" as const, at: version.revision, folder: path },
  }));
  return [
    heading,
    listing,
    element("h3", {}, `${versions.length} revision${versions.length === 1 ? "" : "s"} wrote what is here`),
    timeline(app, route.at, entries, current),
  ];
}

export function showFiles(app: App, route: Route & { view: "files" }) {
  if (!app.byId.has(route.at)) {
    app.left.replaceChildren(error(`no revision ${route.at}`));
    app.right.replaceChildren();
    return;
  }
  let files: TreeFile[];
  try {
    files = filesAt(app, route.at);
  } catch (failure) {
    app.left.replaceChildren(error(failure));
    app.right.replaceChildren();
    return;
  }
  const root = nest(files);
  const selected = route.file ? files.find((file) => file.file === route.file) : undefined;
  // Open every folder above the selection, so it can be seen.
  let above = selected ? parentOf(selected.path) : (route.folder ?? "");
  while (above) {
    opened.add(above);
    above = parentOf(above);
  }

  const scrolled = app.left.querySelector(".tree")?.parentElement?.scrollTop ?? 0;
  app.left.replaceChildren(timeBar(app, route), element("div", { class: "treebox" }, treeList(app, route, root, route.file ?? null)));
  app.left.querySelector(".treebox")!.scrollTop = scrolled;
  app.left.querySelector(".tree .selected")?.scrollIntoView({ block: "nearest" });

  try {
    app.right.replaceChildren(...(route.file ? showFile(app, route, route.file) : showFolder(app, route, root)));
  } catch (failure) {
    app.right.replaceChildren(error(failure));
  }
}
