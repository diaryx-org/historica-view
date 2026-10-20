-- Owners in the path: a repository is /<owner>/<slug>, where the owner is
-- an account's handle. The spike's repositories, each a /<slug> claimed by
-- its first push, are dropped rather than moved: nothing at those addresses
-- is carried over, and their files in R2 are left to be deleted by hand.
drop table "repository";

-- An account's handle: better-auth's `username` plugin, as src/auth.ts
-- configures it. scripts/schema.ts prints these columns as part of "user";
-- SQLite adds a column without `unique`, so an index holds it.
alter table "user" add column "username" text;
alter table "user" add column "displayUsername" text;
create unique index "user_username_unique" on "user" ("username");

-- A repository. Its id names its objects in R2 and its Durable Object, so
-- it is never reused: `autoincrement` keeps a later repository from finding
-- a deleted one's.
create table "repository" (
  "id" integer primary key autoincrement,
  "owner" text not null references "user" ("id") on delete cascade,
  "slug" text not null,
  "private" integer not null default 0,
  "forkedFrom" integer references "repository" ("id") on delete set null,
  "createdAt" date not null,
  unique ("owner", "slug")
);

create index "repository_forkedFrom_idx" on "repository" ("forkedFrom");

-- The accounts an owner added to a repository. The owner is always a
-- member, and has no row here.
create table "member" (
  "repository" integer not null references "repository" ("id") on delete cascade,
  "user" text not null references "user" ("id") on delete cascade,
  "createdAt" date not null,
  primary key ("repository", "user")
);

create index "member_user_idx" on "member" ("user");

-- The digests a repository holds: pushed to it with their bytes, or given
-- it by the repository it was forked from. Its listings name nothing else.
-- `listed` is whether its manifest named the digest when it was last
-- pushed, which is what a fork of it is given.
create table "held" (
  "repository" integer not null references "repository" ("id") on delete cascade,
  "digest" text not null,
  "listed" integer not null default 0,
  primary key ("repository", "digest")
);

create index "held_digest_idx" on "held" ("digest");
