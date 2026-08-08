# LMUKit development guide

This file is the repository handoff for human and LLM-assisted development. Keep it accurate when architecture, commands, formats, or invariants change.

## Product intent

LMUKit is a native Windows toolkit for focused, dependable Le Mans Ultimate utilities. It currently includes binding-profile and companion-launcher tools, while leaving room for other LMU features. The primary runtime platform is Windows; development commonly happens in WSL.

## Architecture

- `src/main.rs` configures the portrait viewport and starts eframe.
- `src/app.rs` owns egui state, the Profiles and Settings tabs, interaction flows, dialogs, and status messages.
- `src/storage.rs` owns settings, profile persistence, imports, activation, backups, LMU JSON parsing, and companion-launcher generation.
- `src/input.rs` reads Windows raw game-controller state and converts buttons, POVs, and axes to LMU input IDs. Its non-Windows implementation keeps native development and tests working.
- `mise.toml` pins tooling and defines the supported developer commands.
- `.github/workflows/release.yml` builds and publishes the Windows x86-64 archive for `v*` tags.
- `build.rs` embeds the executable icon and metadata during native Windows builds. It intentionally skips resource compilation for WSL cross-builds, where `rc.exe` is unavailable.

Keep filesystem and parsing behavior in `storage.rs`; keep platform input details in `input.rs`; keep presentation state in `app.rs`.

## Important invariants

- Never replace LMU's live configuration without first creating a recovery backup when a live file exists.
- Treat imported files as untrusted input. Validate JSON before storing or activating it.
- Compare parsed `serde_json::Value` values for dirty detection so formatting and object-key order do not create false positives.
- Preserve backward compatibility for `settings.json` with `#[serde(default)]` or an explicit migration.
- Do not inspect, modify, or commit a developer's real profiles or Windows application data.
- Keep non-Windows tests and checks functional even when adding Windows-only behavior.
- Do not silently edit Steam configuration. Generate and copy an explicit launch option for the user.

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
```

Settings track the LMU path, active profile UUID, and companion applications. Backups are timestamped under `backups/`.

## Required checks

Run all of these before handing off a code change:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo check --target x86_64-pc-windows-msvc
```

Use `mise run dev` to cross-compile and launch the Windows debug executable from WSL. Use `mise run build` for a release executable.

## Change discipline

- Keep commits small and describe one user-visible or architectural change per commit.
- Preserve unrelated working-tree changes.
- Add or update tests for storage, migration, parsing, and generated launcher behavior.
- Verify Windows-target compilation for changes in `input.rs`, platform launch behavior, or Windows dependencies.
- Update `README.md` and this file when commands or user-facing behavior change.

## Future work

The likely route for car/class detection is LMU's built-in `Local\\LMUSharedMem` mapping. Player scoring exposes vehicle name, class, and vehicle filename. Before implementing automatic activation, determine whether LMU reloads `direct input.json` safely after session/car selection; detection alone does not prove that live replacement is safe.
