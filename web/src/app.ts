// What every view is handed: the open store, the two panes, and the way to
// move somewhere else.

import type { Viewer } from "../pkg/historica_view.js";
import type { Graph, RevisionSummary } from "./types";

export type Route =
  /// The front page: the README, and how to take a copy.
  | { view: "overview"; at: string }
  /// `against` is what the revision is compared with: its first parent when
  /// absent, except for a merge, which then shows how its sides met.
  | { view: "history"; id: string; against?: string }
  /// A file is followed by its identifier, so it is the same file at any
  /// revision whatever it was called there; a folder is only ever a path.
  | { view: "files"; at: string; file?: string; folder?: string }
  /// The task tracker as `at` has it; `task` is a file identifier, as a file
  /// is in Files, so a task stays itself across a move to the closed shelf.
  | { view: "tasks"; at: string; closed?: boolean; task?: string };

/// Where the store came from, as the page tells a reader to take a copy.
export interface Origin {
  /// What the header shows.
  label: string;
  /// The manifest's address, for a published copy.
  manifest?: URL;
  /// The exported folder the manifest sits beside, which names the git
  /// mirror published next to it.
  slug?: string;
}

export interface App {
  viewer: Viewer;
  origin: Origin;
  graph: Graph;
  byId: Map<string, RevisionSummary>;
  left: HTMLElement;
  right: HTMLElement;
  go(route: Route): void;
}

export function href(route: Route): string {
  if (route.view === "overview") return `#overview/${route.at}`;
  if (route.view === "tasks") {
    if (route.task) return `#tasks/${route.at}/t/${route.task}`;
    return `#tasks/${route.at}${route.closed ? "/closed" : ""}`;
  }
  if (route.view === "history") return `#history/${route.id}${route.against ? `/vs/${route.against}` : ""}`;
  if (route.file) return `#files/${route.at}/f/${route.file}`;
  return `#files/${route.at}/d/${encodeURI(route.folder ?? "")}`;
}

export function parse(hash: string): Route | null {
  const parts = hash.replace(/^#/, "").split("/");
  if (parts[0] === "history" && parts[1]) {
    return parts[2] === "vs" && parts[3] ? { view: "history", id: parts[1], against: parts[3] } : { view: "history", id: parts[1] };
  }
  if (parts[0] === "overview" && parts[1]) return { view: "overview", at: parts[1] };
  if (parts[0] === "tasks" && parts[1]) {
    if (parts[2] === "t" && parts[3]) return { view: "tasks", at: parts[1], task: parts[3] };
    return { view: "tasks", at: parts[1], closed: parts[2] === "closed" };
  }
  if (parts[0] === "files" && parts[1]) {
    if (parts[2] === "f" && parts[3]) return { view: "files", at: parts[1], file: parts[3] };
    return { view: "files", at: parts[1], folder: decodeURI(parts.slice(3).join("/")) };
  }
  // The first version of the page put a bare revision in the hash.
  if (/^[0-9a-f]{64}$/.test(parts[0])) return { view: "history", id: parts[0] };
  return null;
}

/// Every revision `at` descends from, and `at` itself.
export function ancestry(app: App, at: string): Set<string> {
  const seen = new Set<string>();
  const stack = [at];
  while (stack.length) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const parent of app.byId.get(id)?.parents ?? []) stack.push(parent);
  }
  return seen;
}
