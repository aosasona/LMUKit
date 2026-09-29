# LMUKit development guide

This file is the repository handoff for human and LLM-assisted development. Keep it accurate when architecture, commands, formats, or invariants change.

## Product intent

LMUKit is a native Windows toolkit for focused, dependable Le Mans Ultimate utilities. It currently includes binding-profile, game-settings, and companion-launcher tools, while leaving room for other LMU features. The primary runtime platform is Windows; development commonly happens in WSL.

## Architecture

- `frontend/src/App.tsx` composes the desktop shell and coordinates Tauri-backed workspace actions.
- `frontend/src/pages/` contains one component per workspace page, `frontend/src/components/` contains shared shell components, and `frontend/src/models.ts` owns the frontend command payload types. Keep page-specific presentation out of `App.tsx`.
- `frontend/src/styles.css` owns the shared visual system. Format TypeScript, JSX, and CSS with Prettier after UI changes.
- `src-tauri/` owns the Tauri desktop shell and narrow command adapters. It must not duplicate storage rules.
- `crates/lmukit-core/` exposes compatibility-sensitive Rust behavior independently of the UI runtime.
- `src/storage.rs` currently backs `lmukit-core` and owns LMUKit settings, profile persistence, imports, activation, backups, LMU JSON parsing, `Settings.JSON` persistence, and companion-launcher generation.
- `src/input.rs` and `src/hotkeys.rs` contain the existing Windows integrations to migrate behind Tauri commands during the rewrite.
- `mise.toml` pins tooling and defines the supported developer commands.
- `.github/workflows/release.yml` builds and publishes the Windows x86-64 archive for `v*` tags.
- `build.rs` embeds the executable icon and metadata during native Windows builds. It intentionally skips resource compilation for WSL cross-builds, where `rc.exe` is unavailable.

Keep filesystem and parsing behavior in `lmukit-core`; keep platform input details in Rust; keep presentation and transient interaction state in React. The previous egui UI is preserved on `legacy/egui-v0.2` and must not be copied into the new frontend component-for-component.

## Important invariants

- Never replace LMU's live configuration without first creating a recovery backup when a live file exists. Likewise, back up a saved profile before updating it from the live configuration.
- Treat imported files as untrusted input. Validate JSON before storing or activating it.
- Preserve unknown profile JSON fields when editing. Only generate mappings for devices already represented in the profile document.
- Preserve unknown `Settings.JSON` values and its `Option#` description fields. Back up the live file before every save.
- Group detected FFB settings by their owning device. LMU's `Steering effects strength` uses a raw `0..10000` value and is presented as a `0..100%` gain control.
- Compare parsed `serde_json::Value` values for dirty detection so formatting and object-key order do not create false positives.
- Preserve backward compatibility for `settings.json` with `#[serde(default)]` or an explicit migration.
- Do not inspect, modify, or commit a developer's real profiles or Windows application data.
- Use the `LMUKIT_DATA_DIR` environment override with synthetic data for screenshots and isolated manual testing.
- Keep non-Windows tests and checks functional even when adding Windows-only behavior.
- Do not silently edit Steam configuration. Generate and copy an explicit launch option for the user.
- LMU does not hot-reload `direct input.json`. Profile activation while LMU is closed prepares the next launch; do not present file replacement as an in-session switch.

## LMU binding format

The managed file is normally:

```text
Le Mans Ultimate/UserData/player/direct input.json
```

Bindings live under the `Input` and `Alternative Input` JSON objects. Each action maps to a device key and numeric `id`. Device metadata is under `Devices`; its `product guid` carries the USB product and vendor IDs.

LMU input IDs currently follow this layout:

- axes: IDs `0..15`, with positive/negative directions paired;
- POV directions: IDs `16..31`;
- buttons: ID `32 + zero-based button index`.

If LMU changes this format, update parser tests and verify against a current file before changing the input conversion.

## Storage model

Each profile has a UUID directory containing:

```text
profiles/<uuid>/profile.json
profiles/<uuid>/direct input.json
profiles/<uuid>/wheel-image.<png|jpg|webp>  # optional
```

Profile metadata may reference an optional managed wheel image; never retain an external source path. Settings track the LMU path, active profile UUID, and companion applications. Timestamped backups under `backups/` preserve both replaced live configurations and saved profiles updated from LMU.

## Required checks

Run all of these before handing off a code change:

```sh
cargo fmt --all --check
cargo test -p lmukit-core
cargo clippy -p lmukit-core --all-targets --all-features -- -D warnings
pnpm build
cargo xwin check -p lmukit --target x86_64-pc-windows-msvc
```

Use `mise run dev` for Tauri development and `mise run build` for a release bundle. WSL cross-checks the Windows backend with `cargo xwin`; native Windows builds embed resources and produce the distributable.

## Change discipline

- Keep commits small and describe one user-visible or architectural change per commit.
- Preserve unrelated working-tree changes.
- Add or update tests for storage, migration, parsing, and generated launcher behavior.
- Verify Windows-target compilation for changes in `input.rs`, platform launch behavior, or Windows dependencies.
- Update `README.md` and this file when commands or user-facing behavior change.

## Future work

The likely route for car/class detection is LMU's built-in `Local\\LMUSharedMem` mapping. Player scoring exposes vehicle name, class, and vehicle filename. LMU does not reload `direct input.json` during a running session, so detection should recommend a mapped profile or prepare it before the next launch rather than promising live automatic switching.

After the Settings UI is migrated, refine the existing generated launcher into a launch-mode and companion-app workflow. Users should be able to select normal or VR LMU startup and independently enable RaceLab, Crew Chief, LMUFFB, or arbitrary executables. Keep paths and enablement backward-compatible with existing `companion_apps` settings, quote executable paths safely, avoid duplicate processes, and test generated scripts with spaces and shell metacharacters. Continue to generate a reviewable Steam launch option; never edit Steam configuration silently.
