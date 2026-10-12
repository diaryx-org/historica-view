// Small helpers both views share.

import type { Hunk } from "./types";

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  for (const [key, value] of Object.entries(attributes)) made.setAttribute(key, value);
  made.append(...children);
  return made;
}

/// A link that calls `go` instead of following its `href`.
export function link(text: string, href: string, go: () => void): HTMLAnchorElement {
  const anchor = element("a", { href }, text);
  anchor.addEventListener("click", (event) => {
    event.preventDefault();
    go();
  });
  return anchor;
}

export const short = (id: string) => id.slice(0, 10);
export const title = (message: string) => message.trim().split("\n")[0] || "(no message)";
export const who = (author: string) => author.replace(/\s*<[^>]*>$/, "");

export function ago(epoch: number): string {
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

export function bytes(size: number): string {
  if (size < 1024) return `${size} bytes`;
  const units = ["KB", "MB", "GB"];
  let value = size / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

export function error(failure: unknown): HTMLElement {
  return element("p", { class: "error" }, String(failure instanceof Error ? failure.message : failure));
}

/// A unified diff, with both sides' line numbers.
export function hunksTable(hunks: Hunk[]): HTMLElement {
  const table = element("table", { class: "diff" });
  for (const hunk of hunks) {
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

/// A file's text with line numbers.
export function textTable(text: string): HTMLElement {
  const table = element("table", { class: "diff content" });
  const lines = text.endsWith("\n") ? text.slice(0, -1).split("\n") : text.split("\n");
  lines.forEach((line, index) => {
    table.append(
      element(
        "tr",
        {},
        element("td", { class: "number" }, String(index + 1)),
        element("td", { class: "text" }, line),
      ),
    );
  });
  return table;
}
