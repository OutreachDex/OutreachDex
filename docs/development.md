# Development

Notes for building, testing and releasing OutreachDex. For what the app does, see the [README](../README.md).

> The project is in early development and is being renamed from its working name, GlobalContact. Some identifiers (binary name, config file name `globalcontact.toml`, crate names, the `globalcontact` CLI) still use the old name.

## Building from source

The contact table and the profile editor are prebuilt editor components that are not published yet, so building from source currently requires their artifacts next to this repository. They will be published with releases so anyone can build the app. Until then, use the prebuilt app from [Releases](https://github.com/OutreachDex/OutreachDex/releases).

Requires Rust 1.95 (via rustup), Node 22, pnpm 10 and the macOS Tauri prerequisites. The Makefile puts `${CARGO_HOME:-$HOME/.cargo}/bin` first on `PATH` so Tauri uses the toolchain from `rust-toolchain.toml`.

```sh
make install
make               # run the desktop app in development mode (Ctrl+C to stop)
make help          # list available targets
pnpm build:local   # ad-hoc signed .app
pnpm bundle:check  # verify the bundled Claude / Codex adapters start
```

`scripts/prepare-kits.mjs` imports the editor artifacts and records each one's version, source revision and artifact hashes in `provenance.json`. Point it at other checkouts or builds with `KIT_TABLE_ROOT`, `KIT_TABLE_DIST`, `KIT_TABLE_BIN` and the matching `KIT_NOTE_*` variables.

## Tests

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm test:flow            # browser flow tests (Chromium); pnpm test:flow:webkit for WebKit
node web/tests/outreach.mjs
```

Browser tests replace only the Tauri IPC and system dialogs; files, services and both editors are real. Mail tests use a local SMTP server and fixed messages and never send externally. To try data sources locally, run the mock backend with `node scripts/gcsp-mock.mjs`, then connect `http://127.0.0.1:8787` with token `dev-token` on the Sources page.

## Release builds

`.github/workflows/release.yml` builds signed, notarized DMGs for Apple Silicon and Intel. Run it from the Actions tab to get the DMGs as run artifacts, or push a tag `vX.Y.Z` matching the version in `src-tauri/tauri.conf.json` to attach them to a draft release. CI fetches the editor components at the revisions pinned in `scripts/editor-revs.txt`; bump those to ship newer editors. The required secrets are listed at the top of the workflow.

## Technical documents

| Document | Contents |
|---|---|
| [gcsp.md](gcsp.md) | GCSP/1, the read-only data source protocol, with [JSON Schema](../spec/gcsp/v1/) and [Go](gcsp-examples/go/main.go) / [Node](../scripts/gcsp-mock.mjs) reference implementations |
| [architecture.md](architecture.md) | Crates, processes and how the editors are embedded |
| [data-model.md](data-model.md) | Workspace layout and tables |
| [agent-bridge.md](agent-bridge.md) | MCP tools available to the Agent |
| [outreach-console](https://github.com/OutreachDex/outreach-console) | Source of outreachdex.com (Astro on Cloudflare) |
