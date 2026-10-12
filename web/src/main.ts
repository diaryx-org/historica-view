import init, { Files, Viewer } from "../pkg/historica_view.js";
import { layout } from "./graph";

interface RevisionSummary {
  id: string;
  change: string;
  parents: string[];
  supersedes: string[];
  author: string;
  when: string;
  epoch: number;
  message: string;
  head: boolean;
  superseded: boolean;
  divergent: boolean;
}

interface NameSummary {
  name: string;
  kind: "revision" | "change" | "file";
  target: string;
  private: boolean;
  revision: string | null;
}

interface Graph {
  revisions: RevisionSummary[];
  names: NameSummary[];
}

interface Line {
  sign: " " | "-" | "+";
  text: string;
}

interface Hunk {
  old_start: number;
  new_start: number;
  lines: Line[];
}

interface FileChange {
  file: string;
  from: string | null;
  to: string | null;
  kind: "added" | "dropped" | "edited" | "moved" | "mode";
  hunks: Hunk[] | null;
  mode: [string, string] | null;
}

interface RevisionDetail {
  id: string;
  against: string | null;
  files: FileChange[];
}

const ROW = 30;
const LANE = 16;
const PALETTE = ["#4f7cff", "#e5732e", "#2ea36b", "#b44fd8", "#d64545", "#1f9fb0", "#b58a1b"];

const pane = document.querySelector<HTMLElement>("#history")!;
const detail = document.querySelector<HTMLElement>("#detail")!;

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  for (const [key, value] of Object.entries(attributes)) made.setAttribute(key, value);
  made.append(...children);
  return made;
}

const short = (id: string) => id.slice(0, 10);
const title = (message: string) => message.trim().split("\n")[0] || "(no message)";
const who = (author: string) => author.replace(/\s*<[^>]*>$/, "");

function ago(epoch: number): string {
  const seconds = Date.now() / 1000 - epoch;
  const units: [number, string][] = [
    [31536000, "y"],
    [2592000, "mo"],
    [86400, "d"],
    [3600, "h"],
    [60, "m"],
  ];
  for (const [size, unit] of units) if (seconds >= size) return `${Math.floor(seconds / size)}${unit} ago`;
  return "just now";
}

async function load(): Promise<{ root: string; viewer: Viewer }> {
  const listing = await fetch("/store/files");
  if (!listing.ok) throw new Error(await listing.text());
  const { root, files: paths } = (await listing.json()) as { root: string; files: string[] };

  const files = new Files();
  const queue = [...paths];
  const worker = async () => {
    for (let path = queue.shift(); path !== undefined; path = queue.shift()) {
      const response = await fetch(`/store/raw/${path.split("/").map(encodeURIComponent).join("/")}`);
      if (!response.ok) throw new Error(`${path}: ${response.status}`);
      files.add(path, new Uint8Array(await response.arrayBuffer()));
    }
  };
  await Promise.all(Array.from({ length: 16 }, worker));
  return { root, viewer: Viewer.open(files) };
}

function renderGraph(graph: Graph, select: (id: string) => void): Map<string, HTMLElement> {
  const rows = graph.revisions;
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
  for (const name of graph.names) {
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
    item.addEventListener("click", () => select(revision.id));
    items.set(revision.id, item);
    list.append(item);
  }

  pane.replaceChildren(element("div", { class: "graph" }, svg, list));
  return items;
}

function renderHunks(change: FileChange): Node {
  if (change.hunks === null) {
    return element("p", { class: "quiet" }, change.kind === "dropped" ? "Dropped." : "Not lines: bytes or a link.");
  }
  const table = element("table", { class: "diff" });
  for (const hunk of change.hunks) {
    table.append(
      element("tr", { class: "hunk" }, element("td", { colspan: "3" }, `@@ −${hunk.old_start} +${hunk.new_start} @@`)),
    );
    let [old, now] = [hunk.old_start, hunk.new_start];
    for (const line of hunk.lines) {
      const left = line.sign === "+" ? "" : String(old++);
      const right = line.sign === "-" ? "" : String(now++);
      const kind = line.sign === "+" ? "insert" : line.sign === "-" ? "delete" : "equal";
      table.append(
        element(
          "tr",
          { class: kind },
          element("td", { class: "number" }, left),
          element("td", { class: "number" }, right),
          element("td", { class: "text" }, `${line.sign} ${line.text}`),
        ),
      );
    }
  }
  return table;
}

function renderDetail(revision: RevisionSummary, change: RevisionDetail, select: (id: string) => void) {
  const [first, ...rest] = revision.message.trim().split("\n");
  const body = rest.join("\n").trim();
  const link = (id: string) => {
    const anchor = element("a", { href: `#${id}` }, short(id));
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      select(id);
    });
    return anchor;
  };

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
  if (revision.parents.length) {
    facts.append(element("dt", {}, revision.parents.length > 1 ? "parents" : "parent"));
    const parents = element("dd", {});
    revision.parents.forEach((parent, index) => parents.append(index ? " " : "", link(parent)));
    facts.append(parents);
  }
  if (revision.supersedes.length) {
    facts.append(element("dt", {}, "supersedes"));
    const superseded = element("dd", {});
    revision.supersedes.forEach((id, index) => superseded.append(index ? " " : "", link(id)));
    facts.append(superseded);
  }

  const files = element("div", { class: "files" });
  const against = change.against
    ? element("p", { class: "quiet" }, "Compared with ", link(change.against), revision.parents.length > 1 ? ", its first parent." : ".")
    : element("p", { class: "quiet" }, "A root: everything here is new.");
  for (const file of change.files) {
    const path =
      file.from && file.to && file.from !== file.to ? `${file.from} → ${file.to}` : (file.to ?? file.from ?? file.file);
    const extra = file.mode ? ` (${file.mode[0]} → ${file.mode[1]})` : "";
    files.append(
      element(
        "details",
        { open: "" },
        element("summary", {}, element("span", { class: `kind ${file.kind}` }, file.kind), " ", path, extra),
        renderHunks(file),
      ),
    );
  }
  if (change.files.length === 0) files.append(element("p", { class: "quiet" }, "No file changed."));

  detail.replaceChildren(
    element("h2", {}, first || "(no message)"),
    ...(body ? [element("pre", { class: "message" }, body)] : []),
    facts,
    against,
    files,
  );
  detail.scrollTop = 0;
}

async function main() {
  await init();
  let loaded: { root: string; viewer: Viewer };
  try {
    loaded = await load();
  } catch (error) {
    pane.replaceChildren(element("p", { class: "error" }, String(error instanceof Error ? error.message : error)));
    return;
  }
  const { root, viewer } = loaded;
  const graph = JSON.parse(viewer.graph()) as Graph;
  document.querySelector("#store")!.textContent = root;
  document.querySelector("#names")!.replaceChildren(
    ...graph.names.map((name) => element("span", { class: "name", title: `${name.kind} ${name.target}` }, name.name)),
  );

  const byId = new Map(graph.revisions.map((revision) => [revision.id, revision]));
  let items = new Map<string, HTMLElement>();
  const select = (id: string) => {
    const revision = byId.get(id);
    if (!revision) return;
    for (const [other, item] of items) item.classList.toggle("selected", other === id);
    items.get(id)?.scrollIntoView({ block: "nearest" });
    if (location.hash !== `#${id}`) history.replaceState(null, "", `#${id}`);
    try {
      renderDetail(revision, JSON.parse(viewer.revision(id)) as RevisionDetail, select);
    } catch (error) {
      detail.replaceChildren(element("p", { class: "error" }, String(error)));
    }
  };
  items = renderGraph(graph, select);

  document.addEventListener("keydown", (event) => {
    if (event.key !== "j" && event.key !== "k" && event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const ids = graph.revisions.map((revision) => revision.id);
    const at = ids.indexOf(location.hash.slice(1));
    const step = event.key === "j" || event.key === "ArrowDown" ? 1 : -1;
    const next = ids[Math.min(ids.length - 1, Math.max(0, at + step))];
    if (next) {
      event.preventDefault();
      select(next);
    }
  });

  const wanted = location.hash.slice(1);
  select(byId.has(wanted) ? wanted : (graph.revisions[0]?.id ?? ""));
}

main();
