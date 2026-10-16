#!/bin/sh
# Publish a Historica repository as a site any static host can serve.
#
#   ./publish.sh <repository> <site> [<target>]
#
# <repository> is a folder with a store in `history/`. <site> is the folder to
# publish, made if it is missing and brought up to date if it is not.
# <target> is what to publish, `head` unless given.
#
# The site holds, side by side:
#
#   index.html, assets/   this view, which reads the rest from beside itself
#   <slug>/               `historica export`: the folder and its history
#   offer.txt             `historica offer --into`: the paged manifest
#   offer-pages/          the pages it names
#   <slug>.git/           a bare git mirror, for `git clone` over plain HTTP
#
# <slug> is the repository folder's name, or $SLUG. A reader takes a copy with
# `historica fetch <url>/offer.txt` or `git clone <url>/<slug>.git`, and the
# page offers both.
#
# Every step is incremental. `export` writes onto the copy it made before and
# withdraws what is no longer there; `offer --into` writes one page of what
# changed; the git mirror is rewritten by `historica-git write`, which moves
# only the refs it made, and pushed into the bare repository. The working git
# repository that write keeps is not part of the site: it lives in
# $HISTORICA_VIEW_STATE, by default under the user's cache directory.
#
# Needs `historica`, `historica-git` and `git` on PATH (or $HISTORICA and
# $HISTORICA_GIT), and a built page in web/dist (`pnpm --dir web build`).

set -eu

usage() {
  sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
repository=$(cd "$1" && pwd)
mkdir -p "$2"
site=$(cd "$2" && pwd)
target=${3:-head}
slug=${SLUG:-$(basename "$repository")}
historica=${HISTORICA:-historica}
historica_git=${HISTORICA_GIT:-historica-git}
here=$(cd "$(dirname "$0")" && pwd)

case $slug in
  */* | .* | "") echo "publish: '$slug' cannot name a folder of the site; set SLUG" >&2; exit 2 ;;
esac
[ -f "$repository/history/historica.txt" ] || {
  echo "publish: $repository has no store in history/" >&2
  exit 1
}
[ -f "$here/web/dist/index.html" ] || {
  echo "publish: no built page; run \`pnpm --dir web build\` first" >&2
  exit 1
}

# 1. The copy and its manifest, which decision 0052 publishes in place of the
# store itself.
"$historica" -C "$repository" export "$site/$slug" "$target"
"$historica" -C "$site" offer "$slug" --into offer.txt

# 2. The git mirror.
state=${HISTORICA_VIEW_STATE:-${XDG_CACHE_HOME:-$HOME/.cache}/historica-view}
work="$state/$(printf '%s' "$site" | cksum | cut -d' ' -f1)-$slug"
mkdir -p "$state"
"$historica_git" write "$site/$slug" "$work"
bare="$site/$slug.git"
[ -d "$bare" ] || git init --quiet --bare "$bare"
git -C "$work" -c push.negotiate=false push --quiet --mirror "$bare"
# The default branch a clone checks out: `main` where there is one.
if git -C "$bare" show-ref --verify --quiet refs/heads/main; then
  git -C "$bare" symbolic-ref HEAD refs/heads/main
fi
# What dumb HTTP serves in place of a git server.
git -C "$bare" update-server-info

# 3. The page.
cp -R "$here/web/dist/." "$site/"

echo "published $repository to $site"
echo "  historica fetch <url>/offer.txt"
echo "  git clone <url>/$slug.git"
