# KiCodex Implementation Plan

## Context

KiCodex is a tool that serves KiCad component libraries via the HTTP Library API, backed by git-trackable CSV files. The full architecture is documented in `AGENTS.md`. This plan covers the phased implementation from MVP to full-featured tray app.

## Phase 1: Minimum Viable HTTP Server

**Goal:** A Rust CLI that reads CSV + YAML files from a directory and serves them to KiCad via the HTTP Library API. No tray app, no multi-project support — just `kicodex serve ./path/to/library`.

### 1.1 Project Structure

Create a cargo workspace:

```
KiCodex/
  Cargo.toml                    # workspace: members = ["kicodex-core", "kicodex-cli"]
  kicodex-core/                 # Library crate
    src/
      lib.rs
      server.rs                 # Axum router + shared state
      models.rs                 # API response types
      routes/
        mod.rs
        root.rs                 # GET /v1/
        categories.rs           # GET /v1/categories.json
        parts.rs                # GET /v1/parts/category/{id}.json + /v1/parts/{id}.json
      data/
        mod.rs
        csv_loader.rs           # CSV parsing, ID management, write-back
        schema.rs               # YAML schema parsing + inheritance
        library.rs              # library.yaml parsing
  kicodex-cli/                  # Binary crate
    src/
      main.rs                   # CLI: `kicodex serve <path>`
  tests/
    fixtures/sample-library/    # Test fixture with library.yaml, schemas, CSV data
```

`src-tauri/` is NOT created yet — added in Phase 3.

### 1.2 Key Dependencies (kicodex-core)

- `axum` 0.8 — HTTP framework
- `tokio` 1 (full features) — async runtime
- `serde` 1, `serde_json` 1 — serialization
- `serde_yml` — YAML parsing (maintained fork of deprecated serde_yaml)
- `csv` 1 — CSV reading/writing
- `indexmap` 2 — ordered maps (preserve field order)
- `thiserror` 2 — error types
- `uuid` 1 (v4 feature) — ID generation
- `tower-http` 0.6 (trace feature) — request logging
- `tracing` + `tracing-subscriber` — logging

For kicodex-cli: `clap` 4 (derive feature).

### 1.3 Implementation Steps (ordered)

1. Create cargo workspace and crate scaffolding
2. **Schema parsing** (`data/schema.rs`): Parse `_base.yaml` and type-specific YAML schemas, resolve inheritance (base fields + type fields merged). Unit tests.
3. **Library manifest** (`data/library.rs`): Parse `library.yaml` to get table-to-schema-to-CSV mappings. Unit tests.
4. **CSV loading** (`data/csv_loader.rs`): Load CSV files, handle ID assignment (generate missing IDs, detect duplicates, write back to CSV immediately). Use temp file + atomic rename for safe writes. Unit tests for round-trip fidelity.
5. **API models** (`models.rs`): Response structs matching KiCad's expected JSON format. All values as strings.
6. **Routes**: Implement all 4 endpoints, with the field mapping logic centralized (see AGENTS.md "Mapping from CSV/Schema to API" table).
7. **Server** (`server.rs`): Wire routes together with shared `LoadedLibrary` state.
8. **CLI** (`main.rs`): `kicodex serve <library-path> [--port 18734]`
9. **Test fixtures**: Create a sample library with `library.yaml`, `_base.yaml`, `resistor.yaml`, and `resistors.csv` (3-5 rows).
10. **Integration tests**: Spin up server on random port, make HTTP requests, assert responses.
11. **Manual test with KiCad**: Hand-write a `.kicad_httplib`, verify components appear in KiCad's symbol chooser.

### 1.4 CSV Write-Back Safety

When writing IDs back to CSV:
- Read with `csv` crate, preserving column order
- Write to a temp file in the same directory
- Atomic rename to replace original
- Preserve original delimiter and quoting settings
- This ensures minimal git diffs

---

## Phase 2: Multi-Project Token Routing + `kicodex init`

**Goal:** Support multiple simultaneous projects via auth token routing. Add the `init` CLI command.

### Key additions:
- **Token registry** (`DashMap<String, RegisteredProject>`) mapping tokens to library paths + loaded data
- **Auth middleware**: Extract `Authorization: Token <value>` header, look up project, return 401 if unknown
- **`kicodex init` command**: Read `kicodex.yaml`, generate UUID token, register with server via internal API (`POST /internal/register`), write `.kicad_httplib`
- **Persistent registry**: Save token mappings to `config_dir/kicodex/projects.json`, reload on server start
- **File watching** (`notify` 7 crate): Watch CSV/YAML files, hot-reload library data on change
- **`kicodex serve`** (no args): Serve all registered projects from persistent registry

### New dependencies:
- `dashmap` 6, `notify` 7, `dirs` 6

---

## Phase 3: Project Discovery + Tray App

**Goal:** Auto-detect open KiCad projects; system tray app.

### Project Discovery Engine:
- **Process scanning**: Find KiCad processes, extract `.kicad_pro` paths from command line args
  - Linux: `/proc/<pid>/cmdline`
  - macOS: `sysinfo` crate
  - Windows: `sysinfo` crate or WMI
- **Lock file watching**: Monitor known project dirs for `.lck` files via `notify`
- Platform-specific code behind a `ProcessScanner` trait with compile-time selection (`#[cfg(target_os)]`)

### Tray App (Tauri v2):
- Add `src-tauri/` to workspace
- System tray with menu: status, active projects list, open config, quit
- No webview window — tray menu only
- Optional start-at-login via `tauri-plugin-autostart`
- Embeds kicodex-core server + discovery engine

### Auto-registration pipeline:
1. Discovery finds new project → check for `kicodex.yaml`
2. If found → auto-register, generate `.kicad_httplib`
3. Detect token collisions (copied folders) → regenerate
4. Prompt user before modifying `.kicad_pro`

### New dependencies:
- `tauri` 2, `tauri-plugin-autostart` 2, `sysinfo` 0.33+

---

## Phase 4: Validation + CI/CD

### `kicodex validate` command:
- Required fields present in CSV headers
- No empty values in required columns
- ID column present and unique
- `kicad_symbol` fields match `Library:Symbol` format
- `url` fields are valid URLs
- JSON output mode (`--json`) for CI

### KiCad library reference validation:
- Parse `sym-lib-table` / `fp-lib-table` (s-expression format)
- Verify symbol/footprint references in CSV exist in installed KiCad libraries
- Warning-level (not blocking)

### GitHub Actions:
- **CI workflow** (push/PR): `cargo fmt --check`, `cargo clippy`, `cargo test` on all 3 platforms
- **Release workflow** (tags): `tauri-apps/tauri-action` for cross-platform builds → GitHub Release with .msi, .dmg, .deb, .AppImage

---

## Phase 5: GUI Library Editor + Symbol/Footprint Picker

### Table editor (Tauri webview):
- Browse/add/edit/delete components
- Schema-aware form fields
- Writes directly to CSV via Tauri IPC commands

### Symbol/footprint picker:
- Parse KiCad's `sym-lib-table` / `fp-lib-table` (global + project-specific)
- Searchable list of available symbols/footprints
- Returns `Library:Symbol` string to editor

### Tray enhancements:
- Warning badge for validation issues
- Per-project submenu actions

---

## Verification Plan

### Phase 1 verification:
1. Run `cargo test` — all unit and integration tests pass
2. Run `kicodex serve tests/fixtures/sample-library/`
3. Create `.kicad_httplib` pointing to `http://127.0.0.1:18734`
4. Open KiCad project → Symbol Chooser → verify "Resistors" category appears with correct components
5. Place a component from the HTTP library → verify fields populate correctly

### Phase 2 verification:
1. Run `kicodex init` in two different project directories
2. Both projects should have different tokens in their `.kicad_httplib` files
3. Open both projects in KiCad simultaneously
4. Verify each sees the correct library version from its submodule

### Phase 3 verification:
1. Start tray app, open a KiCad project by double-clicking `.kicad_pro`
2. Tray app should auto-detect and register the project
3. KiCad should be able to use the HTTP library without manual `kicodex init`
