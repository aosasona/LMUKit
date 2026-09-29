> [!WARNING]
> **LLM usage disclosure:** LMUKit is developed with substantial assistance from large language models. LLM-generated code and documentation are reviewed and tested before being committed, but users should evaluate the software accordingly.

# LMUKit

LMUKit is a modern Windows toolkit for Le Mans Ultimate. It brings profiles, control mappings, game settings, and companion tools into one focused race workspace.

<img src="assets/icon/lmukit.png" alt="LMUKit icon" width="128">

The interface is being rebuilt with Tauri and React. Existing LMUKit profiles, settings, and recovery backups remain compatible; the previous egui application is preserved on the `legacy/egui-v0.2` branch while features move into the new workspace.

The rewrite currently includes the responsive workspace, profile discovery, validated preset import and drag/drop, live-binding capture and update, profile deletion and folder reveal, device-grouped binding browsing, live wheel-control lookup across profiles, search, and safe profile activation. Binding remapping, game-settings editing, global hotkeys, and companion setup remain available in the legacy branch and are the next workflows to migrate before the Tauri version replaces the current release.

## Features

- Capture LMU's current `direct input.json` as a named profile.
- Import preset JSON files or profile folders with drag and drop.
- Activate a saved profile with an automatic recovery backup.
- Assign `Ctrl+Alt+1` through `Ctrl+Alt+9` to profiles for direct switching.
- Open a compact searchable profile switcher with `Ctrl+Alt+Space`.
- Detect and warn when the live bindings differ from the active profile.
- Update an existing profile from LMU's live bindings while backing up its previous version.
- Browse and edit LMU's `Settings.JSON` with inline descriptions from matching `Option#` fields.
- Display the active preset separately from the currently selected row.
- Create reusable wheels with a brand, model, and managed PNG, JPEG, or WebP image, then assign the same wheel to multiple profiles.
- Select any combination of built-in WEC/ELMS classes and optional custom tags per profile before browsing them in a filterable card grid.
- Adjust the application-wide text scale from the Settings tab; the preference is saved between launches.
- Browse and search every action stored in a profile.
- Press a wheel button, move a POV, or turn an axis to compare its mapping across profiles.
- Open the managed profile directory for manual backup or copying.
- Launch LMUKit, LMUFFB, Crew Chief, or other selected companion apps with LMU through one Steam launch option.
- Support custom LMU installations and non-default Steam libraries.

LMUKit stores its own profiles in the current Windows user's application-data directory. Use **Show in folder** to open the exact location.

## Using LMUKit

1. Close LMU before changing profiles.
2. Open **Settings** and confirm the path to LMU's live `direct input.json`.
3. Configure the wheel in LMU and enter a name under **Capture current bindings**.
4. Select a profile and choose **Activate selected** when changing setups.

LMU does not reload its bindings file while running. Close LMU before activating a profile; LMUKit installs the selected file for the next game launch and creates a recovery backup first.

### Faster profile switching

Open **Settings → Profile keyboard shortcuts** to assign `Ctrl+Alt+1` through `Ctrl+Alt+9` to individual profiles. Shortcut assignments are unique, so assigning a number to a different profile moves it automatically.

Press `Ctrl+Alt+Space` anywhere in Windows to open the compact switcher. Search by profile name, use the arrow keys to move, and press Enter to activate the highlighted profile. Direct shortcuts and the compact switcher both prepare the selected profile for the next LMU launch; they cannot change bindings in a running LMU session.

Existing preset files can be dropped directly onto the saved-profile panel. A file is named from its filename; a folder should contain `direct input.json`.

To inspect a physical control, choose **Find wheel button…** and press or move it. LMUKit will show the corresponding action—or **Not mapped**—for every saved profile.

### Profile editor

Select a profile and choose **Edit profile…** to open the Editor tab. The editor can:

- Change or clear primary and alternate mappings by listening for a connected control.
- Warn when the same device input is assigned more than once.
- Edit force-feedback fields grouped by device, including a percentage-based **FFB gain** control for LMU's `Steering effects strength` value.
- Preview the complete resulting JSON while preserving fields LMUKit does not recognise.

**Save** updates the stored profile and backs up its previous version. **Save and activate** also installs it as LMU's live profile using the normal recovery-backup process. LMUKit warns before navigation or app closure would discard editor changes.

For safety, LMUKit only assigns controls from devices already represented in the profile. Capture the device in LMU first if it is not recognised.

### Game settings

Open **Game settings** to edit LMU's `UserData/player/Settings.JSON` without working directly in a text editor. LMUKit supports boolean, numeric, and string options, groups nested settings, and displays a matching `Option#` field as the option's description instead of exposing it as another setting.

Use the search field to find text in names or descriptions. Saving preserves unsupported JSON data and creates a recovery backup first. Close LMU before saving because the game may overwrite its settings while running.

### Companion apps

Open **Settings**, expand **Launch companion apps with LMU**, choose each executable, and enable the apps you want. **Copy Steam launch option** creates a small launcher and copies the required command. Paste it into:

**Steam → Le Mans Ultimate → Properties → Launch Options**

The launcher avoids opening duplicate LMUKit or companion-app processes.

## Development

The repository uses [mise](https://mise.jdx.dev/) to pin Rust, Node.js, pnpm, and developer commands. The desktop shell is Tauri 2, the interface is React and TypeScript, and compatibility-sensitive filesystem logic lives in the independent `lmukit-core` Rust crate.

```sh
mise trust
mise install
pnpm install
mise run dev
```

Useful tasks:

| Command          | Purpose                                       |
| ---------------- | --------------------------------------------- |
| `mise run dev`   | Launch the Tauri development app              |
| `mise run build` | Build the frontend and Windows desktop bundle |
| `mise run test`  | Run Rust compatibility-layer tests            |
| `mise run check` | Check the Rust workspace                      |
| `mise run fmt`   | Check Rust formatting                         |
| `mise run lint`  | Run Clippy with warnings denied               |
| `pnpm build`     | Type-check and build the React interface      |

Release output is written to:

```text
target/release/lmukit.exe
```

Pushing a version tag such as `v0.1.0` runs the Windows release workflow. It creates a GitHub Release containing a versioned x86-64 ZIP and its SHA-256 checksum.

```sh
git tag v0.1.0
git push origin v0.1.0
```

Before committing a change, run:

```sh
cargo fmt --all --check
cargo test -p lmukit-core
cargo clippy -p lmukit-core --all-targets --all-features -- -D warnings
pnpm build
cargo xwin check -p lmukit --target x86_64-pc-windows-msvc
```

Set `LMUKIT_DATA_DIR` to an isolated directory when manually testing or capturing screenshots without using your normal profiles and settings.

## AI-assisted development

LMUKit is developed with LLM assistance. Generated code and documentation are treated like any other contribution: changes should be scoped, reviewed, tested on both the native and Windows targets, and committed in small units with descriptive messages.

Repository-specific guidance for future human and LLM contributors is in [AGENTS.md](AGENTS.md).

## Planned work

LMU exposes player vehicle and class information through its built-in Windows shared-memory interface. Because LMU does not hot-reload `direct input.json`, future detection should recommend or prepare a profile for the next launch rather than swapping controls during a running session.

The Settings migration will also replace the current companion-launcher form with a clearer launch setup. It should let the user choose an LMU launch mode, including the normal desktop mode and VR, then independently enable companion applications such as RaceLab, Crew Chief, LMUFFB, and custom executables. LMUKit must continue to generate an explicit Steam launch option for the user to review and copy rather than modifying Steam configuration itself.

## License

MIT
