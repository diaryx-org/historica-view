import init, { type Viewer } from "../pkg/historica_view.js";
import { type App, href, type Origin, parse, type Route } from "./app";
import { element, error } from "./dom";
import { showFiles } from "./files";
import { showHistory, step } from "./history";
import { load } from "./load";
import { showOverview } from "./overview";
import { showTasks } from "./tasks";
import type { Graph } from "./types";

const left = document.querySelector<HTMLElement>("#left")!;
const right = document.querySelector<HTMLElement>("#right")!;

async function main() {
  await init();
  left.replaceChildren(element("p", { class: "quiet loading" }, "Fetching the history…"));
  let loaded: { origin: Origin; viewer: Viewer };
  try {
    loaded = await load();
  } catch (failure) {
    left.replaceChildren(error(failure));
    return;
  }
  const { origin, viewer } = loaded;
  const graph = JSON.parse(viewer.graph()) as Graph;
  document.querySelector("#store")!.textContent = origin.label;
  document.querySelector("#names")!.replaceChildren(
    ...graph.names.map((name) => element("span", { class: "name", title: `${name.kind} ${name.target}` }, name.name)),
  );

  const newest = graph.revisions.find((revision) => revision.head)?.id ?? graph.revisions[0]?.id ?? "";
  let route: Route = { view: "overview", at: newest };
  // Where each view was last, so switching tabs returns there.
  const last: Record<Route["view"], Route> = {
    overview: { view: "overview", at: newest },
    files: { view: "files", at: newest, folder: "" },
    history: { view: "history", id: newest },
    tasks: { view: "tasks", at: newest },
  };

  const show = () => {
    last[route.view] = route;
    for (const tab of document.querySelectorAll<HTMLAnchorElement>("#tabs a")) {
      tab.classList.toggle("on", tab.dataset.view === route.view);
      tab.href = href(last[tab.dataset.view as Route["view"]]);
    }
    document.body.dataset.view = route.view;
    if (route.view === "history") showHistory(app, route);
    else if (route.view === "files") showFiles(app, route);
    else if (route.view === "tasks") showTasks(app, route);
    else showOverview(app, route);
  };

  const app: App = {
    viewer,
    origin,
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
