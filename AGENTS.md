# LMUKit development guide

This file is the repository handoff for human and LLM-assisted development. Keep it accurate when architecture, commands, formats, or invariants change.

## Product intent

LMUKit is a native Windows toolkit for focused, dependable Le Mans Ultimate utilities. It currently includes binding-profile, game-settings, and companion-launcher tools, while leaving room for other LMU features. The primary runtime platform is Windows; development commonly happens in WSL.

## Architecture

- `frontend/src/App.tsx` composes the desktop shell and coordinates Tauri-backed workspace actions.
- `frontend/src/pages/` contains one component per workspace page, `frontend/src/components/` contains shared shell and editor controls, and `frontend/src/models.ts` owns the frontend command payload types. Keep page-specific presentation out of `App.tsx`. The profile editor composes binding capture, device-grouped FFB controls, and the read-only JSON preview. Virtual button assignment and per-profile live-routing controls belong to the selected profile's detail pane through `ProfileVirtualMappings`; the Virtual Input page owns shared vJoy setup and global routing status only.
- `frontend/src/styles.css` owns the shared visual system. Format TypeScript, JSX, and CSS with Prettier after UI changes.
- `src-tauri/` owns the Tauri desktop shell and narrow command adapters. It must not duplicate storage rules. `src-tauri/src/diagnostics.rs` provides the local rotating logger; the Settings diagnostics panel consumes privacy-safe status and report commands from the shell.
- `crates/lmukit-core/` exposes compatibility-sensitive Rust behavior independently of the UI runtime.
- `crates/lmukit-virtual/` owns the button-only mapping engine, stable logical-action layout, hold-modifier precedence, profile-local digital encoder modes, bounded serialised encoder pulses, replaceable virtual-output boundary, mock backend, continuous DirectInput button monitor, experimental runtime-loaded vJoy adapter, and isolated Windows feasibility probe. The engine selects the most-specific satisfied chord per trigger and uses physical-source ownership so one source cannot release an output still held by another. Encoder detents queue 32 ms virtual presses separated by at least 8 ms released; mode controls cycle once per press and emit no virtual output. The engine must clear queued pulses, reconcile modifier changes, and release outputs on profile changes and shutdown. The crate must not install a driver or alter LMU configuration.
- vJoy is the selected external dependency for Virtual Input Mode. Users install and configure it explicitly; LMUKit must not bundle, install, update, or silently configure the driver. Legacy JSON mode must remain usable without vJoy.
- `src/storage.rs` currently backs `lmukit-core` and owns LMUKit settings, profile persistence, imports, activation, backups, LMU JSON parsing, `Settings.JSON` persistence, wheel/class virtual-profile defaults, and companion-launcher generation.
- `src/input.rs` reads Windows Gaming Input state for the legacy Tauri binding lookup. Initialize each listener on its own fresh OS thread so WinRT apartment mode cannot conflict with the webview thread. Virtual button learning and live routing use the DirectInput monitor in `lmukit-virtual`, because the tested Simagic base is not exposed by Windows Gaming Input. `src/hotkeys.rs` owns global shortcuts; its native manager stays on a dedicated thread that pumps Windows messages and accepts live shortcut updates over a channel.
- The guided virtual-binding setup registers `Ctrl+Alt+V` only for an active setup session. It must allocate against both LMU's saved vJoy bindings and buttons reserved earlier in the session, retry failed pulses, stop on page exit, and never claim LMU accepted an output until a rescan finds it in `direct input.json`.
- `mise.toml` pins tooling and defines the supported developer commands.
- `.github/workflows/release.yml` gates `v*` tags on the required checks, then builds and publishes a portable Windows x86-64 archive, NSIS installer, and checksums.
- `build.rs` embeds the executable icon and metadata during native Windows builds. It intentionally skips resource compilation for WSL cross-builds, where `rc.exe` is unavailable.

Keep filesystem and parsing behavior in `lmukit-core`; keep platform input details in Rust; keep presentation and transient interaction state in React. The previous egui UI is preserved on `legacy/egui-v0.2` and must not be copied into the new frontend component-for-component.

## Important invariants

- Never replace LMU's live configuration without first creating a recovery backup when a live file exists. Likewise, back up a saved profile before updating it from the live configuration.
- Treat imported files as untrusted input. Validate JSON before storing or activating it.
- Preserve unknown profile JSON fields when editing. Only generate mappings for devices already represented in the profile document.
- Preserve unknown `Settings.JSON` values and its `Option#` description fields. Back up the live file before every save.
- Group detected FFB settings by their owning device. LMU's `Steering effects strength` uses a raw `0..10000` value and is presented as a `0..100%` gain control.
- Virtual Input Mode is button-only. It may translate programmable buttons, switches, encoder steps, and optional digital paddles, but it must never read, transform, suppress, or emit steering/pedal axes or participate in the FFB path. Existing profile/settings editing of LMU's stored FFB values is a separate legacy tool.
- The persisted global `input_mode` is authoritative. Legacy profile activation may replace the backed-up LMU JSON only in Legacy mode; live routing may start only in Virtual mode. Switching to Legacy must stop routing and release outputs. Profile shortcuts and the quick switcher must follow the selected mode. Persisting Virtual mode must not automatically start routing after application launch.
- Compare parsed `serde_json::Value` values for dirty detection so formatting and object-key order do not create false positives.
- Preserve backward compatibility for `settings.json` with `#[serde(default)]` or an explicit migration.
- Do not inspect, modify, or commit a developer's real profiles or Windows application data.
- Use the `LMUKIT_DATA_DIR` environment override with synthetic data for screenshots and isolated manual testing.
- Keep non-Windows tests and checks functional even when adding Windows-only behavior.
- Do not silently edit Steam configuration. Generate and copy an explicit launch option for the user.
- VR launch mode must append LMU's native `+XR` argument so it uses OpenXR.
- LMU does not hot-reload `direct input.json`. Profile activation while LMU is closed prepares the next launch; do not present file replacement as an in-session switch.
- Diagnostic logs use local-calendar `lmukit-YYYY-MM-DD.log` names, readable ISO-style UTC timestamps for entries, and retain 5 days by default, configurable from 1 to 30. They may record lifecycle failures and local details needed for troubleshooting, but must not record the input-event stream. Cleanup must not remove exported reports. Exported diagnostic reports must omit configured paths, DirectInput device-instance identifiers, and raw error text that may contain either.

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
wheels/<uuid>/wheel.json
wheels/<uuid>/wheel-image.<png|jpg|webp>  # optional
```

Profiles reference an optional reusable wheel through `wheel_id`; wheel brand, name, and managed image belong to that wheel and are shared by every referencing profile. Existing profile-owned wheel metadata is migrated by matching brand and name, with matching profiles converging on one library entry. Profiles retain `class_tags` and `custom_tags`; empty `class_tags` means a generic profile. The legacy `wheel_tags`, `wheel_brand`, `wheel_name`, and `wheel_image` fields remain readable for migration. Never retain an external image source path. A profile may additionally contain an optional `virtual_mapping` whose physical buttons reference logical actions and whose encoder groups contain two required direction controls, an optional mode-cycle control required for multiple modes, and ordered logical-action mode pairs; absence means it has not been explicitly mapped for Virtual Input Mode. Existing JSON bindings can be explicitly merged into ordinary button mappings by joining on the logical action and translating LMU's product GUID to the live reader's `hid:<vid>:<pid>` device identity; ambiguous sources must remain unmapped for review. Settings track the LMU path, active profile UUID, companion applications, application-wide `ui_font_scale`, default-5 `log_retention_days`, default-Legacy `input_mode`, shared logical-action `virtual_layout`, selected LMU `virtual_device_key`, default-1 vJoy `virtual_device_id`, and unique virtual-profile default references per wheel/class plus optional wheel fallbacks. Default references must be removed when their profile changes wheel, loses the referenced class tag, or is deleted. New settings and profile fields must retain serde defaults for older files. Timestamped backups under `backups/` preserve both replaced live configurations and saved profiles updated from LMU.

## Required checks

Run all of these before handing off a code change:

```sh
cargo fmt --all --check
cargo test -p lmukit-core
cargo clippy -p lmukit-core --all-targets --all-features -- -D warnings
cargo test -p lmukit-virtual
cargo clippy -p lmukit-virtual --all-targets --all-features -- -D warnings
pnpm build
cargo xwin check -p lmukit --target x86_64-pc-windows-msvc
cargo xwin check -p lmukit-virtual --target x86_64-pc-windows-msvc
```

Use `mise run dev` to cross-build and launch the Windows debug app from WSL, and `mise run build` for a Windows release executable. WSL cross-checks the Windows backend with `cargo xwin`; the tagged native-Windows workflow embeds resources and produces the portable archive and NSIS installer.

## Change discipline

- Keep commits small and describe one user-visible or architectural change per commit.
- Preserve unrelated working-tree changes.
- Add or update tests for storage, migration, parsing, and generated launcher behavior.
- Verify Windows-target compilation for changes in `input.rs`, platform launch behavior, or Windows dependencies.
- Update `README.md` and this file when commands or user-facing behavior change.

## Future work

The likely route for car/class detection is LMU's built-in `Local\\LMUSharedMem` mapping. Player scoring exposes vehicle name, class, and vehicle filename. LMU does not reload `direct input.json` during a running session, so detection should recommend a mapped profile or prepare it before the next launch rather than promising live automatic switching.
