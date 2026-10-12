// What every view is handed: the open store, the two panes, and the way to
// move somewhere else.

import type { Viewer } from "../pkg/historica_view.js";
import type { Graph, RevisionSummary } from "./types";

export type Route =
  /// `against` is what the revision is compared with: its first parent when
  /// absent, except for a merge, which then shows how its sides met.
  | { view: "history"; id: string; against?: string }
  /// A file is followed by its identifier, so it is the same file at any
  /// revision whatever it was called there; a folder is only ever a path.
  | { view: "files"; at: string; file?: string; folder?: string };

export interface App {
  viewer: Viewer;
  graph: Graph;
  byId: Map<string, RevisionSummary>;
  left: HTMLElement;
  right: HTMLElement;
  go(route: Route): void;
}

export function href(route: Route): string {
  if (route.view === "history") return `#history/${route.id}${route.against ? `/vs/${route.against}` : ""}`;
  if (route.file) return `#files/${route.at}/f/${route.file}`;
  return `#files/${route.at}/d/${encodeURI(route.folder ?? "")}`;
}

export function parse(hash: string): Route | null {
  const parts = hash.replace(/^#/, "").split("/");
  if (parts[0] === "history" && parts[1]) {
    return parts[2] === "vs" && parts[3] ? { view: "history", id: parts[1], against: parts[3] } : { view: "history", id: parts[1] };
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
