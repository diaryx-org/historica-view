// The hub's accounts, as a worker of their own beside the hub's.
//
// Nothing reaches this worker from the internet: it has no route and no
// workers.dev address, and the hub's worker is the only caller, through its
// `AUTH` service binding. It answers two things:
//
// - `/api/auth/*` — better-auth itself: signing up and in, sessions, an
//   account's handle, and the push keys a signed-in user makes. The hub's
//   worker passes these through as they came, so they are under the hub's
//   own address and its cookies are the hub's. It also asks
//   `/api/auth/get-session` itself, with a reader's cookie, to learn whose
//   it is.
// - `POST /internal/key` — whose push key a bearer token is, which the hub's
//   worker asks before every change a push sends. It never passes a request
//   here from outside. Who owns a repository, and who may read one, the
//   hub's worker reads in D1 itself.

import { hubAuth } from "./auth";
import { mailer, type SendEmail } from "./mail";

interface Env {
  DB: D1Database;
  BETTER_AUTH_SECRET: string;
  /** The hub's own address, as https://hub.example.org. */
  BETTER_AUTH_URL: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
  /** Cloudflare Email Sending, and the address it sends from. */
  EMAIL?: SendEmail;
  EMAIL_FROM?: string;
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
    mail:
      env.EMAIL && env.EMAIL_FROM ? mailer(env.EMAIL, env.EMAIL_FROM) : undefined,
  });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/api/auth/")) {
      return auth(env).handler(request);
    }
    if (url.pathname === "/internal/key" && request.method === "POST") {
      return key(request, env);
    }
    return new Response("not here", { status: 404 });
  },
} satisfies ExportedHandler<Env>;

/**
 * Whose push key `key` is: 200 with the account's id and handle, or 401 when
 * it is not a push key of this hub.
 */
async function key(request: Request, env: Env): Promise<Response> {
  const { key } = await request.json<{ key?: string }>();
  if (!key) {
    return new Response("a key is wanted", { status: 400 });
  }
  const verified = await auth(env).api.verifyApiKey({ body: { key } });
  if (!verified.valid || !verified.key) {
    return new Response(
      "that is not a push key of this hub: make one at /account",
      { status: 401 },
    );
  }
  const account = await env.DB.prepare(
    `select "id", "username", "emailVerified" from "user" where "id" = ?`,
  )
    .bind(verified.key.referenceId)
    .first<{ id: string; username: string | null; emailVerified: number }>();
  if (!account) {
    return new Response("that key's account is gone", { status: 401 });
  }
  // A key made before the hub sent mail pushes only once its account's
  // address is verified, as a key made since could only be.
  if (env.EMAIL && env.EMAIL_FROM && !account.emailVerified) {
    return new Response(
      "that key's account has not verified its email: sign in at /account to be sent a link",
      { status: 401 },
    );
  }
  return Response.json({ id: account.id, username: account.username });
}
