// The hub's accounts: better-auth, over whatever database it is given.
//
// One function so that the worker (over D1) and scripts/schema.ts (over an
// empty SQLite database, to print the tables) build the same configuration,
// and the migrations in ../migrations are the tables this one wants.

import { apiKey } from "@better-auth/api-key";
import { betterAuth, type BetterAuthOptions } from "better-auth";
import { username } from "better-auth/plugins";

export interface Settings {
  database: BetterAuthOptions["database"];
  /** Signs sessions and keys: `BETTER_AUTH_SECRET`. */
  secret: string;
  /** The hub's own address, which every auth URL is under. */
  baseURL: string;
  /** A GitHub OAuth app, when the hub has one. */
  github?: { clientId: string; clientSecret: string };
}

/** The prefix every push key starts with, so a leaked one is recognisable. */
export const KEY_PREFIX = "hub_";

/**
 * Names the hub keeps for its own paths, which no account can take. The
 * hub's worker refuses the same list (`RESERVED` in ../worker/src/route.rs).
 */
const RESERVED = ["account", "api", "client"];

/**
 * Whether `handle` can be an account's: 1 to 39 lowercase letters, digits
 * and inner hyphens, as GitHub's are, and not one of the hub's own names.
 * It is the first segment of every repository's path, `/<handle>/<slug>`, so
 * the hub's worker checks a path against the same rule (`handle_ok`).
 */
export function handleOk(handle: string): boolean {
  return /^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/.test(handle) &&
    !RESERVED.includes(handle);
}

export function hubAuth({ database, secret, baseURL, github }: Settings) {
  return betterAuth({
    database,
    secret,
    baseURL,
    basePath: "/api/auth",
    emailAndPassword: { enabled: true },
    socialProviders: github ? { github } : {},
    telemetry: { enabled: false },
    // better-auth's defaults, stated, because the hub depends on them: the
    // session cookie is unreadable to script, and is not sent with a
    // request from another site. A pushed page runs sandboxed in an origin
    // of its own (the worker's `SANDBOX`), so what it asks of the hub is
    // such a request, and carries no session.
    advanced: {
      defaultCookieAttributes: { httpOnly: true, sameSite: "lax" },
    },
    plugins: [
      // An account's handle, which its repositories are under. Lowercased
      // before it is checked, so `Ada` is `ada`; unique, as better-auth
      // keeps it.
      username({
        minUsernameLength: 1,
        maxUsernameLength: 39,
        usernameValidator: handleOk,
        validationOrder: { username: "post-normalization" },
      }),
      apiKey({
        defaultPrefix: KEY_PREFIX,
        // A push is one request per file it sends, and every one of them
        // checks its key: the plugin's default of ten a day would stop the
        // first push partway.
        rateLimit: { enabled: false },
      }),
    ],
  });
}

export type HubAuth = ReturnType<typeof hubAuth>;
