#!/bin/sh
# Builds kit.table and kit.note from the editor component checkouts (release executables), optionally
# signs the executables, and imports them with prepare-kits.mjs.
#   EDITORS_DIR            editor component checkouts (default: where scripts/ci/fetch-editors.sh puts them)
#   MACOS_SIGNING_IDENTITY Developer ID identity; when set the kit executables are signed with the
#                          hardened runtime and a timestamp, which notarization requires. Tauri does
#                          not sign files under resources/, and copies keep the embedded signature.
# Export KIT_TABLE_BIN and KIT_NOTE_BIN (printed at the end) for the later `tauri build`: its
# beforeBuildCommand runs prepare-kits.mjs again.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
editors="$(cd "${EDITORS_DIR:-$root/../../KitionAI}" && pwd)"

# The engines compile C (SQLite, psm) for wasm32, which Apple's clang cannot target.
if [ -z "${CC_wasm32_unknown_unknown:-}" ]; then
  llvm=""
  for formula in llvm llvm@21 llvm@20 llvm@19 llvm@18; do
    if prefix="$(brew --prefix --installed "$formula" 2>/dev/null)" && [ -x "$prefix/bin/clang" ]; then llvm="$prefix"; break; fi
  done
  if [ -z "$llvm" ]; then brew install llvm; llvm="$(brew --prefix llvm)"; fi
  export CC_wasm32_unknown_unknown="$llvm/bin/clang" AR_wasm32_unknown_unknown="$llvm/bin/llvm-ar"
fi
echo "wasm32 C compiler: $CC_wasm32_unknown_unknown"

pnpm --dir "$editors/kition-tessera" install --frozen-lockfile
pnpm --dir "$editors/kition-sdk" install --frozen-lockfile
for name in table note; do
  repo="$editors/kition-$name"
  pnpm --dir "$repo/web" install --frozen-lockfile
  (cd "$repo" && cargo build --release --locked -p "kition-$name")
  pnpm --dir "$repo/web" engine
  pnpm --dir "$repo/web" build
  if [ -n "${MACOS_SIGNING_IDENTITY:-}" ]; then
    codesign --force --options runtime --timestamp --sign "$MACOS_SIGNING_IDENTITY" "$repo/target/release/kition-$name"
    codesign --verify --strict --verbose=2 "$repo/target/release/kition-$name"
  fi
done

export KIT_TABLE_BIN="$editors/kition-table/target/release/kition-table" KIT_NOTE_BIN="$editors/kition-note/target/release/kition-note"
node "$root/scripts/prepare-kits.mjs"
echo "KIT_TABLE_BIN=$KIT_TABLE_BIN"
echo "KIT_NOTE_BIN=$KIT_NOTE_BIN"
