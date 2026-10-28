#!/bin/sh
# Push a Historica repository to a hub.
#
#   HUB_TOKEN=… ./hub/push.sh <repository> <hub> <owner>[/<slug>]
#
# <repository> is a folder with a store in `history/`; <hub> is the hub's
# address, as https://hub.example.org or http://localhost:8787. It is
# published at <hub>/<owner>/<slug>: <owner> is your handle on the hub, and
# <slug> the folder's name unless given. HUB_TOKEN is a push key, made at
# <hub>/account/. The first push makes the repository, private when
# HUB_PRIVATE=1; you can push only to your own. A push is refused where the
# hub has history this copy does not; HUB_FORCE=1 replaces it.
#
# What goes up is what ../publish.sh makes — the static view, the export, its
# paged manifest and the git mirror — plus `history.tar`, the export's
# `history/` as one archive, which is what the hub's Durable Object opens the
# repository from. The hub keeps every file once by its digest, so a push is
# three steps:
#
#   1. the bytes of each file not sent before, to …/blobs/<sha256>;
#   2. the site listing, `<sha256> <path>` for every file but offer.txt;
#   3. offer.txt, the manifest, with the site listing's digest, which moves
#      the repository to what they name if the copy holds every head the
#      hub's does.
#
# The hub takes a file only with its bytes, and never says whether it had
# them already. What was sent is remembered here, beside the site, in
# $HISTORICA_VIEW_STATE, so a push sends what changed since the last one;
# where the hub is missing something this remembers sending, it says so and
# the push sends everything again.

set -eu

usage() {
  sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[ $# -eq 3 ] || usage
: "${HUB_TOKEN:?push.sh needs HUB_TOKEN, a push key made at /account/ on the hub}"
repository=$(cd "$1" && pwd)
hub=${2%/}
case $3 in
  */*) owner=${3%%/*} slug=${3#*/} ;;
  *) owner=$3 slug=$(basename "$repository") ;;
esac
case $owner/$slug in
  */*/* | /* | */) echo "push: '$3' is not <owner>/<slug>" >&2; exit 2 ;;
esac
here=$(cd "$(dirname "$0")" && pwd)
state=${HISTORICA_VIEW_STATE:-${XDG_CACHE_HOME:-$HOME/.cache}/historica-view}
place="$state/hub/$(printf '%s' "$hub" | cksum | cut -d' ' -f1)/$owner/$slug"
site="$place/site"
sent="$place/sent"
mkdir -p "$site"
touch "$sent"
api="$hub/api/repos/$owner/$slug"

SLUG=$slug "$here/../publish.sh" "$repository" "$site" >/dev/null
tar -C "$site" -cf "$site/history.tar" "$slug/history"

# Every file of the site with its digest, but the manifest.
listing=$(mktemp)
trap 'rm -f "$listing" "$listing.digests" "$listing.answer"' EXIT
(cd "$site" && find . -type f ! -name offer.txt | sed 's|^\./||' | sort | while IFS= read -r path; do
  printf '%s %s\n' "$(sha256sum "$path" | cut -d' ' -f1)" "$path"
done) > "$listing"

# A request to the hub's API. Where the hub refuses, its reason is printed,
# and the status is 9 for a 409 — something named that the hub lacks — and
# 1 for anything else.
call() {
  method=$1 url=$2
  shift 2
  status=$(curl -sS -o "$listing.answer" -w '%{http_code}' -X "$method" \
    -H "Authorization: Bearer $HUB_TOKEN" "$@" --url "$url")
  case $status in
    2??) return 0 ;;
  esac
  printf 'push: %s %s: %s\n' "$method" "${url#"$hub"}" "$(cat "$listing.answer")" >&2
  [ "$status" = 409 ] && return 9
  return 1
}

# Make the repository, or check it is ours.
if [ "${HUB_PRIVATE:-}" = 1 ]; then
  call PUT "$api" -H 'content-type: application/json' --data '{"private":true}'
else
  call PUT "$api"
fi

# The bytes of every file not sent before, each digest once.
send_blobs() {
  sort -u -k1,1 "$listing" > "$listing.digests"
  while IFS= read -r line; do
    digest=${line%% *}
    grep -qxF "$digest" "$sent" && continue
    call PUT "$api/blobs/$digest" --data-binary "@$site/${line#* }"
    echo "$digest" >> "$sent"
    uploaded=$((uploaded + 1))
  done < "$listing.digests"
}

uploaded=0
again=0
send_blobs
offer="$api/offer?site=$(sha256sum "$listing" | cut -d' ' -f1)"
[ "${HUB_FORCE:-}" = 1 ] && offer="$offer&force=1"
until call PUT "$api/site" --data-binary "@$listing" &&
  call PUT "$offer" --data-binary "@$site/offer.txt"; do
  failed=$?
  # The hub lacks something this remembers sending: send it all, once.
  [ "$failed" = 9 ] && [ "$again" = 0 ] || exit 1
  echo "push: sending every file again" >&2
  : > "$sent"
  again=1
  send_blobs
done
# What the hub holds now is what this push listed.
cut -d' ' -f1 "$listing" | sort -u > "$sent"

echo "pushed $repository to $hub/$owner/$slug: $uploaded files sent"
echo "  historica fetch $hub/$owner/$slug/offer.txt"
echo "  git clone $hub/$owner/$slug.git"
