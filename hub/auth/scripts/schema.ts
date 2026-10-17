// Print the SQL that creates better-auth's tables, as ../src/auth.ts
// configures it, for a D1 migration:
//
//   node scripts/schema.ts > migrations/0001_accounts.sql
//
// Planned against an empty SQLite database, so it is every table, created.
// Needs Node 24 or newer, for `node:sqlite` as better-auth reads it and for
// running TypeScript as it is.

import { DatabaseSync } from "node:sqlite";
import { getMigrations } from "better-auth/db/migration";
import { hubAuth } from "../src/auth.ts";

const auth = hubAuth({
  database: new DatabaseSync(":memory:"),
  secret: "only-planning-a-schema-with-this-secret",
  baseURL: "http://localhost",
});
const { compileMigrations } = await getMigrations(auth.options);
process.stdout.write(
  "-- better-auth's tables, as src/auth.ts configures it. Written by\n" +
    "-- scripts/schema.ts; regenerate rather than edit.\n\n" +
    (await compileMigrations()) +
    "\n",
);
