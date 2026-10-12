import init, { Files, Viewer } from "../pkg/historica_view.js";
import { type App, href, parse, type Route } from "./app";
import { element, error } from "./dom";
import { showFiles } from "./files";
import { showHistory, step } from "./history";
import type { Graph } from "./types";

const left = document.querySelector<HTMLElement>("#left")!;
const right = document.querySelector<HTMLElement>("#right")!;

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

async function main() {
  await init();
  let loaded: { root: string; viewer: Viewer };
  try {
    loaded = await load();
  } catch (failure) {
    left.replaceChildren(error(failure));
    return;
  }
  const { root, viewer } = loaded;
  const graph = JSON.parse(viewer.graph()) as Graph;
  document.querySelector("#store")!.textContent = root;
  document.querySelector("#names")!.replaceChildren(
    ...graph.names.map((name) => element("span", { class: "name", title: `${name.kind} ${name.target}` }, name.name)),
  );

  const newest = graph.revisions.find((revision) => revision.head)?.id ?? graph.revisions[0]?.id ?? "";
  let route: Route = { view: "history", id: newest };
  // Where each view was last, so switching tabs returns there.
  const last: { history: Route; files: Route } = {
    history: { view: "history", id: newest },
    files: { view: "files", at: newest, folder: "" },
  };

  const show = () => {
    last[route.view] = route;
    for (const tab of document.querySelectorAll<HTMLAnchorElement>("#tabs a")) {
      tab.classList.toggle("on", tab.dataset.view === route.view);
      tab.href = href(last[tab.dataset.view as Route["view"]]);
    }
    document.body.dataset.view = route.view;
    if (route.view === "history") showHistory(app, route.id);
    else showFiles(app, route);
  };

  const app: App = {
    viewer,
    graph,
    byId: new Map(graph.revisions.map((revision) => [revision.id, revision])),
    left,
    right,
    go(next) {
      route = next;
      const hash = href(next);
      if (location.hash !== hash) history.pushState(null, "", hash);
      show();
    },
  };

  for (const tab of document.querySelectorAll<HTMLAnchorElement>("#tabs a")) {
    tab.addEventListener("click", (event) => {
      event.preventDefault();
      app.go(last[tab.dataset.view as Route["view"]]);
    });
  }
  window.addEventListener("popstate", () => {
    route = parse(location.hash) ?? route;
    show();
  });
  document.addEventListener("keydown", (event) => {
    if (route.view !== "history" || event.target instanceof HTMLSelectElement) return;
    const by = event.key === "j" || event.key === "ArrowDown" ? 1 : event.key === "k" || event.key === "ArrowUp" ? -1 : 0;
    const next = by && step(app, route.id, by);
    if (next) {
      event.preventDefault();
      app.go({ view: "history", id: next });
    }
  });

  route = parse(location.hash) ?? route;
  history.replaceState(null, "", href(route));
  show();
}

main();
