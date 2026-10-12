// The revision graph, and what one revision did.

import { type App, href, type Route } from "./app";
import { ago, element, error, hunksTable, link, short, title, who } from "./dom";
import { layout } from "./graph";
import { mergeView } from "./merge";
import type { FileChange, NameSummary, RevisionDetail, RevisionSummary } from "./types";

const ROW = 30;
const LANE = 16;
export const PALETTE = ["#4f7cff", "#e5732e", "#2ea36b", "#b44fd8", "#d64545", "#1f9fb0", "#b58a1b"];

let drawn: { list: HTMLElement; items: Map<string, HTMLElement> } | null = null;

function graphPane(app: App): { list: HTMLElement; items: Map<string, HTMLElement> } {
  if (drawn) return drawn;
  const rows = app.graph.revisions;
  const { columns, edges, width } = layout(rows);
  const graphWidth = Math.max(1, width) * LANE + LANE / 2;
  const x = (lane: number) => LANE / 2 + lane * LANE + 4;
  const y = (row: number) => row * ROW + ROW / 2;
  const colour = (lane: number) => PALETTE[lane % PALETTE.length];

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "lanes");
  svg.setAttribute("width", String(graphWidth));
  svg.setAttribute("height", String(rows.length * ROW));
  const draw = (tag: string, attributes: Record<string, string | number>) => {
    const shape = document.createElementNS("http://www.w3.org/2000/svg", tag);
    for (const [key, value] of Object.entries(attributes)) shape.setAttribute(key, String(value));
    svg.append(shape);
  };

  edges.forEach((below, row) => {
    for (const edge of below) {
      const [x1, y1, x2, y2] = [x(edge.from), y(row), x(edge.to), y(row + 1)];
      const d =
        x1 === x2
          ? `M${x1},${y1} L${x2},${y2}`
          : `M${x1},${y1} C${x1},${y1 + ROW * 0.6} ${x2},${y2 - ROW * 0.6} ${x2},${y2}`;
      draw("path", { d, stroke: colour(edge.lane), fill: "none", "stroke-width": 2 });
    }
  });
  rows.forEach((revision, row) => {
    const merge = revision.parents.length > 1;
    draw("circle", {
      cx: x(columns[row]),
      cy: y(row),
      r: merge ? 4 : 5,
      fill: revision.superseded ? "var(--background)" : colour(columns[row]),
      stroke: colour(columns[row]),
      "stroke-width": merge ? 3 : 2,
    });
  });

  const namesAt = new Map<string, NameSummary[]>();
  for (const name of app.graph.names) {
    if (!name.revision) continue;
    namesAt.set(name.revision, [...(namesAt.get(name.revision) ?? []), name]);
  }

  const list = element("ol", { class: "rows", style: `padding-left:${graphWidth + 8}px` });
  const items = new Map<string, HTMLElement>();
  for (const revision of rows) {
    const marks: (Node | string)[] = [];
    for (const name of namesAt.get(revision.id) ?? []) marks.push(element("span", { class: "name" }, name.name));
    if (revision.head) marks.push(element("span", { class: "mark head" }, "head"));
    if (revision.divergent) marks.push(element("span", { class: "mark divergent" }, "divergent"));
    if (revision.superseded) marks.push(element("span", { class: "mark superseded" }, "superseded"));
    const item = element(
      "li",
      { tabindex: "0", "data-id": revision.id },
      element("code", { class: "id" }, short(revision.id)),
      ...marks,
      element("span", { class: "title" }, title(revision.message)),
      element("span", { class: "author" }, who(revision.author)),
      element("time", { datetime: revision.when, title: revision.when }, ago(revision.epoch)),
    );
    item.addEventListener("click", () => app.go({ view: "history", id: revision.id }));
    items.set(revision.id, item);
    list.append(item);
  }

  drawn = { list: element("div", { class: "graph" }, svg, list), items };
  return drawn;
}

function fileSection(app: App, revision: RevisionSummary, against: string | null, file: FileChange): HTMLElement {
  const path =
    file.from && file.to && file.from !== file.to ? `${file.from} → ${file.to}` : (file.to ?? file.from ?? file.file);
  const extra = file.mode ? ` (${file.mode[0]} → ${file.mode[1]})` : "";
  // A dropped file is not in the folder this revision leaves, so it is
  // looked at in the one before.
  const at = file.kind === "dropped" ? (against ?? revision.id) : revision.id;
  const open = { view: "files" as const, at, file: file.file };
  const body =
    file.hunks === null
      ? element("p", { class: "quiet" }, file.kind === "dropped" ? "Dropped." : "Not lines: bytes or a link.")
      : hunksTable(file.hunks);
  return element(
    "details",
    { open: "" },
    element(
      "summary",
      {},
      element("span", { class: `kind ${file.kind}` }, file.kind),
      " ",
      path,
      extra,
      " ",
      link("history of this file", href(open), () => app.go(open)),
    ),
    body,
  );
}

/// For a merge, a choice between how its sides met and each parent alone.
function compareTabs(app: App, revision: RevisionSummary, against: string | undefined): HTMLElement {
  const tabs = element("div", { class: "tabs" });
  const choices: [string | undefined, string][] = [
    [undefined, "How the sides met"],
    ...revision.parents.map((parent, index): [string, string] => [parent, `Against parent ${index + 1}`]),
  ];
  for (const [choice, label] of choices) {
    const tab = element("button", { type: "button", class: choice === against ? "on" : "" }, label);
    if (choice) tab.title = `${short(choice)} ${title(app.byId.get(choice)?.message ?? "")}`;
    tab.addEventListener("click", () => app.go({ view: "history", id: revision.id, against: choice }));
    tabs.append(tab);
  }
  return element("div", { class: "controls" }, tabs);
}

function changes(app: App, revision: RevisionSummary, change: RevisionDetail): HTMLElement[] {
  const to = (id: string) => link(short(id), href({ view: "history", id }), () => app.go({ view: "history", id }));
  const merge = revision.parents.length > 1;
  const against = change.against
    ? element(
        "p",
        { class: "quiet" },
        "Compared with ",
        to(change.against),
        merge ? `, parent ${revision.parents.indexOf(change.against) + 1}: what the merge brought in from the others.` : ".",
      )
    : element("p", { class: "quiet" }, "A root: everything here is new.");
  const sections = change.files.map((file) => fileSection(app, revision, change.against, file));
  if (!sections.length) sections.push(element("p", { class: "quiet" }, "No file changed."));
  return [against, element("div", { class: "files" }, ...sections)];
}

function detail(app: App, revision: RevisionSummary, route: { id: string; against?: string }): HTMLElement[] {
  const [first, ...rest] = revision.message.trim().split("\n");
  const body = rest.join("\n").trim();
  const to = (id: string) => link(short(id), href({ view: "history", id }), () => app.go({ view: "history", id }));

  const facts = element(
    "dl",
    {},
    element("dt", {}, "revision"),
    element("dd", {}, element("code", {}, revision.id)),
    element("dt", {}, "change"),
    element("dd", {}, element("code", {}, revision.change)),
    element("dt", {}, "author"),
    element("dd", {}, revision.author),
    element("dt", {}, "when"),
    element("dd", {}, revision.when),
  );
  const ids = (label: string, list: string[]) => {
    if (!list.length) return;
    const cell = element("dd", {});
    list.forEach((id, index) => cell.append(index ? " " : "", to(id)));
    facts.append(element("dt", {}, label), cell);
  };
  ids(revision.parents.length > 1 ? "parents" : "parent", revision.parents);
  ids("supersedes", revision.supersedes);
  const files = { view: "files" as const, at: revision.id, folder: "" };
  facts.append(element("dt", {}, "folder"), element("dd", {}, link("browse the files as they were", href(files), () => app.go(files))));

  const merge = revision.parents.length > 1;
  const shown =
    merge && !route.against
      ? mergeView(app, revision)
      : changes(app, revision, JSON.parse(app.viewer.revision(route.id, route.against)) as RevisionDetail);

  return [
    element("h2", {}, first || "(no message)"),
    ...(body ? [element("pre", { class: "message" }, body)] : []),
    facts,
    ...(merge ? [compareTabs(app, revision, route.against)] : []),
    ...shown,
  ];
}

export function showHistory(app: App, route: Extract<Route, { view: "history" }>) {
  const id = route.id;
  const revision = app.byId.get(id);
  const { list, items } = graphPane(app);
  if (app.left.firstChild !== list) app.left.replaceChildren(list);
  for (const [other, item] of items) item.classList.toggle("selected", other === id);
  items.get(id)?.scrollIntoView({ block: "nearest" });
  if (!revision) {
    app.right.replaceChildren(error(`no revision ${id}`));
    return;
  }
  try {
    app.right.replaceChildren(...detail(app, revision, route));
  } catch (failure) {
    app.right.replaceChildren(error(failure));
  }
  app.right.scrollTop = 0;
}

/// The revision `j` or `k` moves to from `id`.
export function step(app: App, id: string, by: number): string | undefined {
  const ids = app.graph.revisions.map((revision) => revision.id);
  const at = ids.indexOf(id);
  return ids[Math.min(ids.length - 1, Math.max(0, at + by))];
}
