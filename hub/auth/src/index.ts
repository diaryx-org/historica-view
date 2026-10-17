// The hub's accounts, as a worker of their own beside the hub's.
//
// Nothing reaches this worker from the internet: it has no route and no
// workers.dev address, and the hub's worker is the only caller, through its
// `AUTH` service binding. It answers two things:
//
// - `/api/auth/*` — better-auth itself: signing up and in, sessions, and the
//   push keys a signed-in user makes. The hub's worker passes these through
//   as they came, so they are under the hub's own address and its cookies
//   are the hub's.
// - `POST /internal/push` — whether a push key may push to a repository,
//   which the hub's worker asks before every object a push sends. It never
//   passes a request here from outside.

import { hubAuth } from "./auth";

interface Env {
  DB: D1Database;
  BETTER_AUTH_SECRET: string;
  /** The hub's own address, as https://hub.example.org. */
  BETTER_AUTH_URL: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
}

function auth(env: Env) {
  return hubAuth({
    database: env.DB,
    secret: env.BETTER_AUTH_SECRET,
    baseURL: env.BETTER_AUTH_URL,
    github:
      env.GITHUB_CLIENT_ID && env.GITHUB_CLIENT_SECRET
        ? {
            clientId: env.GITHUB_CLIENT_ID,
            clientSecret: env.GITHUB_CLIENT_SECRET,
          }
        : undefined,
  });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/api/auth/")) {
      return auth(env).handler(request);
    }
    if (url.pathname === "/internal/push" && request.method === "POST") {
      return push(request, env);
    }
    return new Response("not here", { status: 404 });
  },
} satisfies ExportedHandler<Env>;

/**
 * Whether `key` may push to `slug`. A repository belongs to whoever first
 * pushed to it, so a slug no one holds is claimed by this push's owner.
 *
 * 204 when it may; 401 when the key is not one, and 403 when it is someone
 * else's repository, each with a reason push.sh can print.
 */
async function push(request: Request, env: Env): Promise<Response> {
  const { key, slug } = await request.json<{ key?: string; slug?: string }>();
  if (!key || !slug) {
    return new Response("a key and a slug are wanted", { status: 400 });
  }
  const verified = await auth(env).api.verifyApiKey({ body: { key } });
  if (!verified.valid || !verified.key) {
    return new Response(
      "that is not a push key of this hub: make one at /account",
      { status: 401 },
    );
  }
  const user = verified.key.referenceId;
  await env.DB.prepare(
    `insert into "repository" ("slug", "owner", "createdAt") values (?, ?, ?)
     on conflict ("slug") do nothing`,
  )
    .bind(slug, user, new Date().toISOString())
    .run();
  const owner = await env.DB.prepare(
    `select "owner" from "repository" where "slug" = ?`,
  )
    .bind(slug)
    .first<string>("owner");
  if (owner !== user) {
    return new Response(`${slug} belongs to someone else on this hub`, {
      status: 403,
    });
  }
  return new Response(null, { status: 204 });
}
