// The dev server hands the page one local store, read-only.
//
// A browser cannot walk a directory, so the server lists it: `/store/files`
// is every file under the store as a JSON array of relative paths, and
// `/store/raw/<path>` is one file's bytes. A hosted copy answers the same two
// questions from its offer manifest instead, and the page cannot tell which
// it is talking to.

import { readdir, readFile, stat } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import type { Plugin } from "vite";
import { defineConfig } from "vite";

/// The store's own folder: the argument itself, or `history/` beneath it.
async function storeRoot(given: string): Promise<string> {
  const path = resolve(given);
  for (const candidate of [path, join(path, "history")]) {
    try {
      await stat(join(candidate, "historica.txt"));
      return candidate;
    } catch {}
  }
  throw new Error(`${path} is neither a Historica store nor a folder with one in history/`);
}

async function walk(root: string, at: string, into: string[]): Promise<void> {
  for (const entry of await readdir(at, { withFileTypes: true })) {
    const path = join(at, entry.name);
    if (entry.isDirectory()) await walk(root, path, into);
    else if (entry.isFile()) into.push(relative(root, path).split(sep).join("/"));
  }
}

function localStore(): Plugin {
  const given = process.env.HISTORICA_STORE ?? "..";
  return {
    name: "historica-local-store",
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        const url = new URL(request.url ?? "/", "http://localhost");
        if (!url.pathname.startsWith("/store/")) return next();
        try {
          const root = await storeRoot(given);
          if (url.pathname === "/store/files") {
            const files: string[] = [];
            await walk(root, root, files);
            response.setHeader("content-type", "application/json");
            response.setHeader("cache-control", "no-store");
            response.end(JSON.stringify({ root, files }));
            return;
          }
          if (url.pathname.startsWith("/store/raw/")) {
            const wanted = decodeURIComponent(url.pathname.slice("/store/raw/".length));
            const path = resolve(root, wanted);
            if (!path.startsWith(root + sep)) {
              response.statusCode = 403;
              response.end("outside the store");
              return;
            }
            response.setHeader("content-type", "application/octet-stream");
            response.setHeader("cache-control", "no-store");
            response.end(await readFile(path));
            return;
          }
          response.statusCode = 404;
          response.end();
        } catch (error) {
          response.statusCode = 500;
          response.end(String(error instanceof Error ? error.message : error));
        }
      });
    },
  };
}

export default defineConfig({
  plugins: [localStore()],
  server: { port: Number(process.env.PORT ?? 5174) },
});
