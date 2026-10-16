// The task tracker, as a revision has it.
//
// A task is a document of the repository, so the tracker at any revision is
// the tracker as it stood then, and a task's history is its file's history.

import { type App, href, type Route } from "./app";
import { element, error, link, short } from "./dom";
import { rendered } from "./markdown";
import type { Task, TaskDetail } from "./types";

const lists = new Map<string, Task[]>();

function tasksAt(app: App, at: string): Task[] {
  let tasks = lists.get(at);
  if (!tasks) {
    tasks = JSON.parse(app.viewer.tasks(at)) as Task[];
    lists.set(at, tasks);
  }
  return tasks;
}

const day = (stamp: string | null) => (stamp ? stamp.slice(0, 10) : "");

function badge(task: Task): HTMLElement {
  return element("span", { class: `status ${task.closed ? "closed" : "open"} ${task.status}` }, task.status);
}

function list(app: App, route: Route & { view: "tasks" }, tasks: Task[]): HTMLElement[] {
  const open = tasks.filter((task) => !task.closed);
  const closed = tasks.filter((task) => task.closed);
  const selected = route.task ? tasks.find((task) => task.file === route.task) : undefined;
  // A selected task decides which half of the list is shown.
  const showClosed = selected ? selected.closed : !!route.closed;

  const toggle = (label: string, count: number, on: boolean, next: Route) => {
    const anchor = link(`${count} ${label}`, href(next), () => app.go(next));
    if (on) anchor.classList.add("on");
    return anchor;
  };
  const bar = element(
    "nav",
    { class: "task-states" },
    toggle("Open", open.length, !showClosed, { view: "tasks", at: route.at }),
    toggle("Closed", closed.length, showClosed, { view: "tasks", at: route.at, closed: true }),
  );

  const shown = showClosed ? closed : open;
  const items = element("ul", { class: "tasks" });
  for (const task of shown) {
    const item = element(
      "li",
      { class: task.file === route.task ? "selected" : "", tabindex: "0" },
      element("div", { class: "task-title" }, badge(task), element("span", {}, task.title)),
      element("div", { class: "quiet task-meta" }, [day(task.created), task.author].filter(Boolean).join(" · ") || task.path),
    );
    item.addEventListener("click", () => app.go({ view: "tasks", at: route.at, task: task.file }));
    items.append(item);
  }
  if (!shown.length) items.append(element("li", { class: "quiet empty" }, showClosed ? "Nothing closed yet." : "Nothing open."));
  return [bar, element("div", { class: "taskbox" }, items)];
}

function detail(app: App, route: Route & { view: "tasks" }, tasks: Task[]): HTMLElement[] {
  if (!route.task) {
    return [
      element("h2", {}, "Tasks"),
      element(
        "p",
        { class: "quiet" },
        "Every document under an index titled Tasks with a status, as prov's tasks preset lays them out. ",
        "Closing one is an edit to its status, so the tracker at any revision is the tracker as it stood then.",
      ),
    ];
  }
  const shown = JSON.parse(app.viewer.task(route.at, route.task)) as TaskDetail | null;
  if (!shown) return [element("p", { class: "quiet" }, "Not a document at this moment.")];
  const file = { view: "files" as const, at: route.at, file: shown.file };
  const facts = element("dl", {});
  const fact = (term: string, value: Node | string | null) => {
    if (value) facts.append(element("dt", {}, term), element("dd", {}, value));
  };
  fact("Status", badge(shown));
  fact("Author", shown.author);
  fact("Created", day(shown.created));
  fact("Updated", day(shown.updated));
  fact("File", link(shown.path, href(file), () => app.go(file)));
  if (shown.description) fact("Summary", shown.description);

  const tasksHere = new Set(tasks.map((task) => task.file));
  return [
    element("h2", {}, shown.title),
    facts,
    element(
      "p",
      { class: "quiet" },
      "Its history is its file's: ",
      link("every version", href(file), () => app.go(file)),
      ` of it up to ${short(route.at)}.`,
    ),
    rendered(app, route.at, shown.html, shown.path, tasksHere),
  ];
}

export function showTasks(app: App, route: Route & { view: "tasks" }) {
  if (!app.byId.has(route.at)) {
    app.left.replaceChildren(error(`no revision ${route.at}`));
    app.right.replaceChildren();
    return;
  }
  let tasks: Task[];
  try {
    tasks = tasksAt(app, route.at);
  } catch (failure) {
    app.left.replaceChildren(error(failure));
    app.right.replaceChildren();
    return;
  }
  app.left.replaceChildren(...list(app, route, tasks));
  try {
    app.right.replaceChildren(...detail(app, route, tasks));
  } catch (failure) {
    app.right.replaceChildren(error(failure));
  }
}
