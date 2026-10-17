-- Who owns each repository on the hub: whoever first pushed to its slug.
-- Only the owner's keys can push to it after.
create table "repository" (
  "slug" text not null primary key,
  "owner" text not null references "user" ("id") on delete cascade,
  "createdAt" date not null
);

create index "repository_owner_idx" on "repository" ("owner");
