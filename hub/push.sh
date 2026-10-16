#!/bin/sh
# Push a Historica repository to a hub.
#
#   HUB_TOKEN=… ./hub/push.sh <repository> <hub> [<slug>]
#
# <repository> is a folder with a store in `history/`; <hub> is the hub's
# address, as https://hub.example.org or http://localhost:8787. <slug> is the
# name it is published under, the folder's name unless given.
#
# What goes up is what ../publish.sh makes — the static view, the export, its
# paged manifest and the git mirror — plus `history.tar`, the export's
# `history/` as one archive, which is what the hub's Durable Object opens the
# repository from. The site is kept between pushes in $HISTORICA_VIEW_STATE,
# so publish.sh brings it up to date rather than rewriting it, and only the
# files that changed since the last push are sent: what was sent is
# remembered beside the site, by digest.
#
# The manifest goes last. The hub reloads a repository when its manifest
# moves, so everything it names is there before it moves; files the copy no
# longer holds are removed after it.

set -eu

usage() {
  sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
: "${HUB_TOKEN:?push.sh needs HUB_TOKEN, the hub's token}"
repository=$(cd "$1" && pwd)
hub=${2%/}
slug=${3:-$(basename "$repository")}
here=$(cd "$(dirname "$0")" && pwd)
state=${HISTORICA_VIEW_STATE:-${XDG_CACHE_HOME:-$HOME/.cache}/historica-view}
site="$state/hub/$slug/site"
sent="$state/hub/$slug/sent"
mkdir -p "$site"
touch "$sent"

SLUG=$slug "$here/../publish.sh" "$repository" "$site" >/dev/null
tar -C "$site" -cf "$site/history.tar" "$slug/history"

# Every file of the site with its digest, the manifest last.
listing=$(mktemp)
trap 'rm -f "$listing" "$listing.new" "$listing.gone"' EXIT
(cd "$site" && find . -type f ! -name offer.txt | sed 's|^\./||' | sort | while IFS= read -r path; do
  printf '%s %s\n' "$(sha256sum "$path" | cut -d' ' -f1)" "$path"
done) > "$listing"
printf '%s %s\n' "$(sha256sum "$site/offer.txt" | cut -d' ' -f1)" offer.txt >> "$listing"

# PUT a file of the site, or DELETE one, by its path within the site.
send() {
  if [ "$1" = PUT ]; then
    set -- "$1" "$2" --data-binary "@$site/$2"
  fi
  method=$1 path=$2
  shift 2
  curl -fsS -X "$method" -H "Authorization: Bearer $HUB_TOKEN" "$@" \
    --url-query "path=$path" "$hub/api/repos/$slug/objects"
}

uploaded=0
while IFS= read -r line; do
  grep -qxF "$line" "$sent" && continue
  send PUT "${line#* }"
  uploaded=$((uploaded + 1))
done < "$listing"

# What the last push sent and this copy no longer holds.
cut -d' ' -f2- "$listing" > "$listing.new"
cut -d' ' -f2- "$sent" | grep -vxF -f "$listing.new" > "$listing.gone" || true
while IFS= read -r path; do
  send DELETE "$path"
done < "$listing.gone"
removed=$(wc -l < "$listing.gone" | tr -d ' ')
cp "$listing" "$sent"

echo "pushed $repository to $hub/$slug: $uploaded files sent, $removed removed"
echo "  historica fetch $hub/$slug/offer.txt"
echo "  git clone $hub/$slug.git"
