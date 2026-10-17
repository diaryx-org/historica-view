// The hub's accounts: better-auth, over whatever database it is given.
//
// One function so that the worker (over D1) and scripts/schema.ts (over an
// empty SQLite database, to print the tables) build the same configuration,
// and the migrations in ../migrations are the tables this one wants.

import { apiKey } from "@better-auth/api-key";
import { betterAuth, type BetterAuthOptions } from "better-auth";

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

export function hubAuth({ database, secret, baseURL, github }: Settings) {
  return betterAuth({
    database,
    secret,
    baseURL,
    basePath: "/api/auth",
    emailAndPassword: { enabled: true },
    socialProviders: github ? { github } : {},
    telemetry: { enabled: false },
    plugins: [
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
