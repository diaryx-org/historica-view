// A document's rendered body, made safe and made to point inside the store.
//
// The view module renders Markdown and passes raw HTML through, because a
// README that lays out its own header should look as its author meant. A
// published repository is somebody else's writing, so DOMPurify takes out
// anything that would run before it reaches the page.
//
// Links and images that point at files of the repository are read against the
// document's own path and the folder as `at` has it: a link to another file
// opens it in Files, a link to a task opens it in Tasks, and an image is shown
// from the store's own bytes rather than fetched from wherever the page is.

import DOMPurify from "dompurify";
import { type App, href, type Route } from "./app";
import { element } from "./dom";
import { filesAt, imageType } from "./files";

/// Whether `reference` points inside the repository rather than at the web.
const internal = (reference: string) => !/^[a-z][a-z0-9+.-]*:/i.test(reference) && !reference.startsWith("//") && !reference.startsWith("#");

/// `reference` read against the document at `from`, as a path from the root.
function resolve(reference: string, from: string): string {
  const [path] = reference.split(/[?#]/);
  const parts = path.startsWith("/") ? [] : from.split("/").slice(0, -1);
  for (const part of decodeURIComponent(path).split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}

export function rendered(app: App, at: string, html: string, from: string, tasks: Set<string> = new Set()): HTMLElement {
  const article = element("article", { class: "markdown" });
  article.append(DOMPurify.sanitize(html, { RETURN_DOM_FRAGMENT: true }));
  const files = filesAt(app, at);
  const byPath = new Map(files.map((file) => [file.path, file]));

  for (const anchor of article.querySelectorAll<HTMLAnchorElement>("a[href]")) {
    const reference = anchor.getAttribute("href")!;
    if (!internal(reference)) {
      if (!reference.startsWith("#")) anchor.setAttribute("rel", "noopener noreferrer");
      continue;
    }
    const path = resolve(reference, from);
    const file = byPath.get(path);
    let route: Route | null = null;
    if (file) route = tasks.has(file.file) ? { view: "tasks", at, task: file.file } : { view: "files", at, file: file.file };
    else if (path === "" || files.some((each) => each.path.startsWith(`${path}/`))) route = { view: "files", at, folder: path };
    if (!route) {
      anchor.removeAttribute("href");
      anchor.title = `${path} is not in the folder at this moment`;
      continue;
    }
    const target = route;
    anchor.href = href(target);
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      app.go(target);
    });
  }

  const local = (reference: string): string | null => {
    if (!internal(reference)) return null;
    const path = resolve(reference, from);
    const file = byPath.get(path);
    const type = imageType(path);
    if (!file || !type) return "";
    const data = app.viewer.fileBytes(at, file.file);
    if (!data) return "";
    return URL.createObjectURL(new Blob([new Uint8Array(data)], { type }));
  };
  for (const image of article.querySelectorAll<HTMLImageElement>("img[src]")) {
    const url = local(image.getAttribute("src")!);
    if (url !== null) image.src = url;
  }
  // `<picture>` offers a dark-mode source beside the image; one URL each is
  // what READMEs write.
  for (const source of article.querySelectorAll<HTMLSourceElement>("source[srcset]")) {
    const url = local(source.getAttribute("srcset")!.trim().split(/\s+/)[0]);
    if (url === "") source.remove();
    else if (url !== null) source.srcset = url;
  }
  return article;
}
