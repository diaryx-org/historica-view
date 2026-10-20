// The front page: what the repository is, how to take a copy, and its README.

import { type App, href, type Route } from "./app";
import { ago, element, error, link, short, title, who } from "./dom";
import { rendered } from "./markdown";
import type { Overview } from "./types";

/// Whether a git mirror sits beside the manifest, answered once.
let mirror: Promise<URL | null> | null = null;

function gitMirror(app: App): Promise<URL | null> {
  if (mirror) return mirror;
  const { manifest, slug } = app.origin;
  if (!manifest || !slug) return (mirror = Promise.resolve(null));
  const url = new URL(`${encodeURIComponent(slug)}.git`, manifest);
  // Dumb HTTP: a bare repository with `git update-server-info` run in it
  // serves `info/refs`, and that is all `git clone` asks a static host for.
  mirror = fetch(new URL(`${url.pathname}/info/refs`, url), { method: "HEAD" })
    .then((response) => (response.ok ? url : null))
    .catch(() => null);
  return mirror;
}

function command(label: string, text: string): HTMLElement {
  const copy = element("button", { type: "button", class: "copy" }, "Copy");
  copy.addEventListener("click", () => {
    navigator.clipboard?.writeText(text).then(() => {
      copy.textContent = "Copied";
      setTimeout(() => (copy.textContent = "Copy"), 1500);
    });
  });
  return element("div", { class: "command" }, element("span", { class: "quiet" }, label), element("code", {}, text), copy);
}

function cloneBox(app: App): HTMLElement {
  const box = element("section", { class: "clone" });
  const { manifest } = app.origin;
  if (!manifest) {
    box.append(element("p", { class: "quiet" }, "A local store. Publish it with ", element("code", {}, "publish.sh"), " to give it an address."));
    return box;
  }
  box.append(command("historica", `historica fetch ${manifest.href}`));
  gitMirror(app).then((url) => {
    if (url) box.append(command("git", `git clone ${url.href}`));
  });
  return box;
}

export function showOverview(app: App, route: Route & { view: "overview" }) {
  app.left.replaceChildren();
  if (!app.byId.has(route.at)) {
    app.right.replaceChildren(error(`no revision ${route.at}`));
    return;
  }
  let overview: Overview;
  try {
    overview = JSON.parse(app.viewer.overview(route.at)) as Overview;
  } catch (failure) {
    app.right.replaceChildren(error(failure));
    return;
  }
  const revision = app.byId.get(route.at)!;
  const latest = app.graph.revisions.find((each) => each.head)?.id;

  const heading = element("h2", { class: "repository" }, overview.title ?? app.origin.slug ?? "Repository");
  const facts = element("p", { class: "facts" });
  const history = { view: "history" as const, id: route.at };
  facts.append(
    link(`${app.graph.revisions.length} revisions`, href(history), () => app.go(history)),
    ` · ${app.graph.names.length} bookmark${app.graph.names.length === 1 ? "" : "s"}`,
  );

  const last = element(
    "p",
    { class: "latest-revision" },
    link(short(revision.id), href(history), () => app.go(history)),
    ` ${title(revision.message)} — ${who(revision.author)}, `,
    element("time", { datetime: revision.when, title: revision.when }, ago(revision.epoch)),
  );
  if (latest && latest !== route.at) {
    const now = { view: "overview" as const, at: latest };
    last.append(" · ", link("see the latest", href(now), () => app.go(now)));
  }

  const parts: HTMLElement[] = [heading];
  if (overview.description) parts.push(element("p", { class: "description" }, overview.description));
  parts.push(facts, cloneBox(app), last);
  if (overview.readme) {
    const readme = overview.readme;
    const file = { view: "files" as const, at: route.at, file: readme.file };
    parts.push(
      element(
        "div",
        { class: "readme" },
        element("div", { class: "readme-name" }, link(readme.path, href(file), () => app.go(file))),
        rendered(app, route.at, readme.html, readme.path),
      ),
    );
  } else {
    parts.push(element("p", { class: "quiet" }, "No README at this moment."));
  }
  app.right.replaceChildren(element("div", { class: "page" }, ...parts));
}
