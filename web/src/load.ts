// Where the store comes from.
//
// Three places, tried in this order:
//
// - `?offer=<url>` names a published copy's manifest anywhere, which is how
//   one page reads every repository published as decision 0052 has it;
// - `offer.txt` beside the page, which is how a published site reads the copy
//   it was published with;
// - the dev server's `/store/files` and `/store/raw/`, which hand the page one
//   local store.
//
// A published copy is read the way `historica fetch` reads it: the manifest,
// the pages it names, and then the files they list, each hashed against the
// digest it was listed under before the store sees it.

import { Files, Manifest, Viewer } from "../pkg/historica_view.js";
import type { Origin } from "./app";
import type { Listing } from "./types";

const WORKERS = 16;

/// Run `each` over `items`, a few at a time.
async function pool<T>(items: T[], each: (item: T) => Promise<void>): Promise<void> {
  const queue = [...items];
  const worker = async () => {
    for (let item = queue.shift(); item !== undefined; item = queue.shift()) await each(item);
  };
  await Promise.all(Array.from({ length: Math.min(WORKERS, items.length) }, worker));
}

const encoded = (path: string) => path.split("/").map(encodeURIComponent).join("/");

async function bytesOf(url: URL | string, what: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${what}: ${response.status} ${response.statusText}`);
  return new Uint8Array(await response.arrayBuffer());
}

function isManifest(text: string): boolean {
  return text.startsWith("historica-offer-");
}

async function published(manifest: URL, text: string): Promise<{ origin: Origin; viewer: Viewer }> {
  const read = Manifest.read(text);
  await pool(read.wanted(), async (page) => {
    read.page(page, await bytesOf(new URL(encoded(page), manifest), page));
  });
  const listing = JSON.parse(read.listing()) as Listing;

  const files = new Files();
  files.add("historica.txt", await bytesOf(new URL(encoded(listing.header), manifest), listing.header));
  await pool(listing.files, async (file) => {
    files.addListed(file.store, await bytesOf(new URL(encoded(file.path), manifest), file.path), file.digest);
  });
  const slug = listing.header.split("/history/")[0];
  return {
    origin: { label: manifest.href, manifest, slug: slug && !slug.includes("/") ? slug : undefined },
    viewer: Viewer.open(files),
  };
}

async function local(): Promise<{ origin: Origin; viewer: Viewer }> {
  const listing = await fetch("/store/files");
  if (!listing.ok) throw new Error(await listing.text());
  const { root, files: paths } = (await listing.json()) as { root: string; files: string[] };
  const files = new Files();
  await pool(paths, async (path) => {
    files.add(path, await bytesOf(`/store/raw/${encoded(path)}`, path));
  });
  return { origin: { label: root }, viewer: Viewer.open(files) };
}

export async function load(): Promise<{ origin: Origin; viewer: Viewer }> {
  const given = new URLSearchParams(location.search).get("offer");
  if (given) {
    const manifest = new URL(given, location.href);
    const response = await fetch(manifest);
    if (!response.ok) throw new Error(`${manifest.href}: ${response.status} ${response.statusText}`);
    return published(manifest, await response.text());
  }
  // A site published with its copy has the manifest beside the page. A dev
  // server answers anything with the page itself, which is not a manifest.
  const beside = new URL("offer.txt", location.href);
  const response = await fetch(beside).catch(() => null);
  if (response?.ok) {
    const text = await response.text();
    if (isManifest(text)) return published(beside, text);
  }
  return local();
}
