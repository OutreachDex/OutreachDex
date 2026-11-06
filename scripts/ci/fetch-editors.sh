#!/bin/sh
# Clones the editor component repositories pinned in scripts/editor-revs.txt next to this repository.
#   EDITORS_DIR       destination (default: the directory the Cargo path dependencies point to)
#   EDITORS_GIT_BASE  clone URL prefix (private, so CI rewrites it to an authenticated URL with
#                     `git config url.<...>.insteadOf`)
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
dest="${EDITORS_DIR:-$root/../../KitionAI}"
base="${EDITORS_GIT_BASE:-https://github.com/KitionAI}"
mkdir -p "$dest"
grep -v '^#' "$root/scripts/editor-revs.txt" | while read -r repo rev; do
  [ -n "$repo" ] || continue
  dir="$dest/$repo"
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" rev-parse HEAD)" = "$rev" ]; then
    echo "$repo already at $rev"
    continue
  fi
  rm -rf "$dir"
  git init -q "$dir"
  git -C "$dir" remote add origin "$base/$repo.git"
  git -C "$dir" fetch -q --depth 1 origin "$rev"
  git -C "$dir" -c advice.detachedHead=false checkout -q FETCH_HEAD
  echo "$repo $rev"
done
