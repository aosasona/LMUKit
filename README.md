# Willa

Willa is a small native Windows app for saving and switching Le Mans Ultimate
wheel-binding profiles. It is written entirely in Rust using egui/eframe.

## Current features

- Uses LMU's standard `direct input.json` location by default
- Captures the current LMU bindings as a named profile
- Activates a profile while preserving its exact filename
- Creates a recovery backup before replacing the live configuration
- Allows a custom LMU location for non-default Steam libraries

Class- and car-based automatic profile selection is planned after the manual
profile workflow. That feature will require a reliable way to identify the
current LMU vehicle (likely shared-memory telemetry or log/session data).

## Development

```sh
cargo run
cargo test
```

For a distributable optimized executable:

```sh
cargo build --release
```

The executable will be at `target/release/willa.exe` when built on Windows.
