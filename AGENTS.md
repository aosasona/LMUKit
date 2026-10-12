# LMUKit development guide

This file is the repository handoff for human and LLM-assisted development. Keep it accurate when architecture, commands, formats, or invariants change.

## Product intent

LMUKit is a native Windows toolkit for focused, dependable Le Mans Ultimate utilities. It currently includes binding-profile, game-settings, and companion-launcher tools, while leaving room for other LMU features. The primary runtime platform is Windows; development commonly happens in WSL.

## Architecture

- `frontend/src/App.tsx` composes the desktop shell, coordinates Tauri-backed workspace actions, and owns the shared live-routing, LMU car-context, and automatic-selection status used by Overview and Profiles. Auto-started or externally switched live profiles must update this shared state and the selected profile so activation controls never present an already-live profile as inactive.
- `frontend/src/pages/` contains one component per workspace page, `frontend/src/components/` contains shared shell and editor controls, and `frontend/src/models.ts` owns the frontend command payload types. Keep page-specific presentation out of `App.tsx`. The profile editor composes binding capture, device-grouped FFB controls, and the read-only JSON preview. Virtual button assignment and per-profile live-routing controls belong to the selected profile's detail pane through `ProfileVirtualMappings`; the Virtual Input page owns shared vJoy setup and global routing status only.
- `frontend/src/pages/HelpPage.tsx` owns the in-app setup guide for both input modes and supported hardware categories. Keep its claims aligned with actual adapters: Virtual Input supports DirectInput-visible digital controls, not controller axes, rumble, POV-only controls, or unverified XInput-only devices.
- `frontend/src/styles.css` owns the shared visual system. Format TypeScript, JSX, and CSS with Prettier after UI changes.
- `src-tauri/` owns the Tauri desktop shell and narrow command adapters. It must not duplicate storage rules. Its LMU car-context worker samples off the UI thread every 1.5 seconds and reopens the game-owned mapping for each read so LMU shutdown cannot leave an apparently live snapshot. It also owns the default-off automatic selector runtime so class switching continues while the window is hidden. `src-tauri/src/diagnostics.rs` provides the local rotating logger; the Settings diagnostics panel consumes privacy-safe status and report commands from the shell.
- Closing the main window hides it in the Windows notification area and must not stop live routing. The tray menu exposes **Show LMUKit** and **Exit LMUKit**; tray and sidebar exit actions must show the same frontend confirmation before requesting application exit. The Tauri exit lifecycle releases virtual outputs before shutdown.
- LMUKit is single-instance. A second launch must restore, unminimize, and focus the existing main window instead of opening a second routing engine or tray icon. Register the single-instance plugin before other Tauri plugins and reuse the shared window-show path.
- `crates/lmukit-core/` exposes compatibility-sensitive Rust behavior independently of the UI runtime.
- `crates/lmukit-context/` owns the read-only LMU car-context snapshot decoder, the Windows `LMU_Data` mapping/lock adapter, synthetic contract tests, and the isolated context probe. It uses explicit little-endian offsets validated against the installed official headers instead of redistributing those proprietary headers or depending on native C++ layout. Keep mapping access off the UI thread, reject incompatible sizes or implausible values, and never guess unknown classes.
- `crates/lmukit-virtual/` owns the button-only mapping engine, stable logical-action layout, hold-modifier precedence, profile-local digital encoder modes, bounded serialised encoder pulses, replaceable virtual-output boundary, mock backend, continuous DirectInput button monitor, experimental runtime-loaded vJoy adapter, and isolated Windows feasibility probe. The engine selects the most-specific satisfied chord per trigger and uses physical-source ownership so one source cannot release an output still held by another. Ordinary taps remain pressed for at least 32 ms so a buffered press/release pair cannot disappear between LMU polls; modifier-driven remaps still release the replaced output immediately. Encoder detents queue 32 ms virtual presses separated by at least 8 ms released; mode controls cycle once per press and emit no virtual output. The engine must clear queued pulses, reconcile modifier changes, and release outputs on profile changes and shutdown. The crate must not install a driver or alter LMU configuration.
- vJoy is the selected external dependency for Virtual Input Mode. Users install and configure device 1 with 128 buttons explicitly; LMUKit must report and warn about a smaller capacity, but must not bundle, install, update, or silently configure the driver. Legacy JSON mode must remain usable without vJoy.
- `src/storage.rs` currently backs `lmukit-core` and owns LMUKit settings, profile persistence, imports, activation, backups, LMU JSON parsing, `Settings.JSON` persistence, global class-to-profile defaults, and companion-launcher generation.
- `src/input.rs` reads Windows Gaming Input state for the legacy Tauri binding lookup. Initialize each listener on its own fresh OS thread so WinRT apartment mode cannot conflict with the webview thread. Virtual button learning and live routing use the DirectInput monitor in `lmukit-virtual`, because the tested Simagic base is not exposed by Windows Gaming Input. The DirectInput reader must retain buffered button transitions, reconcile them against state snapshots, and perform periodic controller discovery away from the latency-sensitive polling thread; snapshot-only polling can lose a complete short paddle press while discovery blocks. `src/hotkeys.rs` owns global shortcuts; its native manager stays on a dedicated thread that pumps Windows messages and accepts live shortcut updates over a channel.
- `src-tauri/src/lmu_process.rs` isolates Windows process enumeration for live-file write guards. Legacy profile activation, activating from the profile editor, and saving live `Settings.JSON` must refuse to write while LMU is running unless the foreground UI obtained explicit confirmation. Detection errors must cancel the write rather than assuming LMU is closed; read-only operations remain unaffected.
- The guided virtual-binding setup registers `Ctrl+Alt+V` only for an active setup session. It must allocate against both LMU's saved vJoy bindings and buttons reserved earlier in the session, retry failed pulses, stop on page exit, and never claim LMU accepted an output until a rescan finds it in `direct input.json`. Finish/rescan automatically persists a validated layout only when exactly one vJoy device has named button bindings; multiple candidates require explicit selection so a stale re-enumerated device is never guessed.
- The shared virtual layout may also contain manually named actions for external applications such as RaceLab. Manual registration allocates and pulses the lowest unused configured vJoy button while live routing is stopped, persists the user-provided name, and exposes it through the existing per-profile mapping editor with normal modifier support. LMU rescans must preserve manual actions and reject button/name conflicts instead of silently replacing them. Prevent removal while any profile button or encoder mode still references the action.
- The virtual mapping workspace's read-only control test uses DirectInput and resolves the most-specific satisfied modifier mapping without writing profile data or emitting vJoy output. Keep it disabled while live routing is active and make cancellation share the existing lookup-generation mechanism. Learning must capture before saving and require explicit confirmation before replacing an exact button/modifier combination assigned to another action; enforce that rule in core storage as well as the UI.
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
- The persisted global `input_mode` is authoritative. Legacy profile activation may replace the backed-up LMU JSON only in Legacy mode; live routing may start only in Virtual mode. Switching to Legacy must stop routing and release outputs. Profile shortcuts and the quick switcher must follow the selected mode. A global top-bar action must stop active virtual routing and release outputs from every page. Live routing remains stopped after launch by default; the explicit default-off `auto_start_virtual_profile` setting may restore only the last successfully routed profile, and only while the persisted mode is Virtual.
- Automatic class selection is separately persisted, default-off, and valid only in Virtual Input mode. It requires two consecutive confirmed normalized-class observations, resolves only an explicit global class default, and leaves the current route untouched for unknown, stale, loading, unsupported, or missing-default states. Manual profile/class selection, profile shortcuts, and manual routing stops pause it until explicit Resume Auto. Keep selection in the Rust background worker so hiding the window does not stop it.
- Compare parsed `serde_json::Value` values for dirty detection so formatting and object-key order do not create false positives.
- Show LMU live-file difference warnings and Update from LMU actions only in Legacy JSON mode. Virtual Input mappings do not use the live JSON swap flow and must not nag about those differences.
- Preserve backward compatibility for `settings.json` with `#[serde(default)]` or an explicit migration.
- Do not inspect, modify, or commit a developer's real profiles or Windows application data.
- Use the `LMUKIT_DATA_DIR` environment override with synthetic data for screenshots and isolated manual testing.
- Keep non-Windows tests and checks functional even when adding Windows-only behavior.
- Do not silently edit Steam configuration. Generate and copy an explicit launch option for the user.
- Generated companion launchers must start LMUKit and every companion executable with that executable's parent directory as its Windows working directory. Some apps, including LMUFFB, otherwise resolve profiles relative to Steam or LMU and appear unconfigured.
- VR launch mode must append LMU's native `+XR` argument so it uses OpenXR.
- LMU does not hot-reload `direct input.json`. Profile activation while LMU is closed prepares the next launch; do not present file replacement as an in-session switch.
- Check whether LMU is running before every live `direct input.json` or `Settings.JSON` write. A failed check must be visible and cancel the operation. Global Legacy shortcuts have no confirmation surface and therefore must refuse while LMU runs.
- Diagnostic logs use local-calendar `lmukit-YYYY-MM-DD.log` names with numbered same-day segments, readable ISO-style UTC timestamps for entries, a 2 MB per-file cap, at most 3 files per day, and retain 5 days by default, configurable from 1 to 30. Individual messages are capped at 8,192 characters. They may record lifecycle failures and local details needed for troubleshooting, but must not record the input-event stream. Cleanup must not remove exported reports. Exported diagnostic reports must omit configured paths, DirectInput device-instance identifiers, and raw error text that may contain either.

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

Profiles reference an optional reusable wheel through `wheel_id`; wheel brand, name, and managed image belong to that wheel and are shared by every referencing profile. Existing profile-owned wheel metadata is migrated by matching brand and name, with matching profiles converging on one library entry. Profiles retain `class_tags` and `custom_tags`; empty `class_tags` means a generic profile. The legacy `wheel_tags`, `wheel_brand`, `wheel_name`, and `wheel_image` fields remain readable for migration. Never retain an external image source path. A profile may additionally contain an optional `virtual_mapping` whose physical buttons reference logical actions and whose encoder groups contain two required direction controls, an optional mode-cycle control required for multiple modes, and ordered logical-action mode pairs; absence means it has not been explicitly mapped for Virtual Input Mode. Existing JSON bindings can be explicitly merged into ordinary button mappings by joining on the logical action and translating LMU's product GUID to the live reader's `hid:<vid>:<pid>` device identity; ambiguous sources must remain unmapped for review. Settings track the LMU path, active profile UUID, companion applications, application-wide `ui_font_scale`, default-5 `log_retention_days`, default-Legacy `input_mode`, default-off `auto_start_virtual_profile`, default-off `auto_profile_selection`, the last successfully routed virtual profile, shared logical-action `virtual_layout`, the subset of manually named external-app actions, selected LMU `virtual_device_key`, default-1 vJoy `virtual_device_id`, and at most one global default profile reference per class. Legacy wheel/class defaults remain readable for migration: migrate a class only when all old references agree on one profile, and never guess across conflicting wheel defaults. Default references must be removed when their profile loses the class tag or is deleted; a deleted last-live profile reference must also be cleared. Changing a profile's wheel does not invalidate a global class default. New settings and profile fields must retain serde defaults for older files. Timestamped backups under `backups/` preserve both replaced live configurations and saved profiles updated from LMU.

## Required checks

Run all of these before handing off a code change:

```sh
cargo fmt --all --check
cargo test -p lmukit-context
cargo clippy -p lmukit-context --all-targets --all-features -- -D warnings
cargo test -p lmukit-core
cargo clippy -p lmukit-core --all-targets --all-features -- -D warnings
cargo test -p lmukit-virtual
cargo clippy -p lmukit-virtual --all-targets --all-features -- -D warnings
pnpm build
cargo xwin check -p lmukit --target x86_64-pc-windows-msvc
cargo xwin check -p lmukit-context --target x86_64-pc-windows-msvc
cargo xwin check -p lmukit-virtual --target x86_64-pc-windows-msvc
cargo xwin test -p lmukit-context --target x86_64-pc-windows-msvc --no-run
cargo xwin test -p lmukit-virtual --target x86_64-pc-windows-msvc --no-run
```

Use `mise run dev` to cross-build and launch the Windows debug app from WSL, and `mise run build` for a Windows release executable. WSL cross-checks the Windows backend with `cargo xwin`; the no-run test build is required because `cargo xwin check` does not catch missing native link libraries. The tagged native-Windows workflow embeds resources and produces the portable archive and NSIS installer.

## Change discipline

- Keep commits small and describe one user-visible or architectural change per commit.
- Preserve unrelated working-tree changes.
- Add or update tests for storage, migration, parsing, and generated launcher behavior.
- Verify Windows-target compilation for changes in `input.rs`, platform launch behavior, or Windows dependencies.
- Update `README.md` and this file when commands or user-facing behavior change.

## Future work

The verified route for car/class detection in the installed LMU v1.4200 build is its native `LMU_Data` mapping under the contract shipped in `Support/SharedMemoryInterface`. The isolated reader and desktop worker expose the current player vehicle, raw class, normalized existing tag, vehicle identifier, session state, provenance, and freshness. Default-off automatic selection requires two matching confirmed observations, resolves one global default per class, ignores unknown/stale context, and pauses after manual selection until Resume Auto. The end-to-end selector and manual override have passed the user's LMU/Simagic/vJoy hardware test. The remaining acceptance scope is a fuller menu/loading/disconnect matrix and future LMU layout changes. Legacy mode never claims an in-session switch because LMU does not reload `direct input.json` during a session.
