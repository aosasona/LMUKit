# Virtual Input Mode: profile and binding model

Virtual Input Mode is an optional, button-only alternative to LMUKit's existing JSON profile switching. It keeps one stable virtual DirectInput controller bound in Le Mans Ultimate while LMUKit changes how physical wheel controls map onto it. Ordinary virtual-profile changes do not replace LMU's `direct input.json` and should not require an LMU restart once the virtual controller has been configured and validated.

The existing Legacy JSON mode remains available and unchanged. Existing profiles, reusable wheels, and class tags are reused by Virtual Input Mode; there is no parallel profile or class system.

## Stable translation model

Each managed input follows one direction:

```text
physical wheel control
    -> active profile mapping
    -> logical action
    -> stable virtual button
    -> LMU binding
```

A logical action is a durable semantic name such as `pit_limiter`, `brake_bias_increase`, or `front_arb_decrease`. The global virtual layout assigns each logical action one stable vJoy button. Profiles map their own physical controls to those logical actions; they do not choose arbitrary virtual button numbers independently.

| Active profile mapping            | Stable virtual layout   | One-time LMU binding |
| --------------------------------- | ----------------------- | -------------------- |
| GT Neo button 5 -> Pit Limiter    | Pit Limiter -> vJoy B1  | B1 -> Pit Limiter    |
| FX Pro button 12 -> Pit Limiter   | Pit Limiter -> vJoy B1  | B1 -> Pit Limiter    |
| GT Neo encoder CW -> Brake Bias + | Brake Bias + -> vJoy B8 | B8 -> Brake Bias +   |

Because the virtual layout is shared, `B1` continues to mean Pit Limiter when the user changes wheel, class, or profile. Profile switching changes only LMUKit's live translation; it does not recreate the virtual controller or rewrite the LMU binding.

## Initial setup

The user installs and explicitly configures vJoy, then binds its virtual buttons in LMU once. Device 1 should be configured for vJoy's full 128-button capacity before LMU or LMUKit starts; the default/minimal eight-button configuration is insufficient for the intended shared action layout. LMUKit reports the actual capacity and warns when it is below 128, but it does not silently reconfigure the driver. After changing the capacity, restart LMU and LMUKit and verify any existing assignments. vJoy is an optional external dependency: LMUKit does not bundle, install, update, or silently configure it. Legacy JSON mode remains fully usable without vJoy.

Only actions that should be managed by LMUKit need virtual bindings. After LMU has saved those bindings, LMUKit reads `direct input.json` and builds the shared layout from the selected virtual device: LMU input ID 32 is vJoy B1, ID 33 is B2, and so on. The LMU action name becomes the stable logical action.

The guided setup lets the user leave LMU listening on an action and press `Ctrl+Alt+V` to trigger LMUKit. The shortcut is registered only while setup is active. LMUKit checks vJoy device 1, reads its actual button capacity, reserves buttons already present in LMU's saved configuration, emits the lowest unused button as a bounded press/release, and reserves each successful pulse in memory so it cannot be reused before LMU writes the file. A failed pulse returns its button to the sequence. An on-screen trigger is available as a fallback.

After LMU saves its assignments, **Finish & rescan** stops the hotkey and reads `direct input.json` again. Emitting a pulse is not treated as proof that LMU accepted it; the resulting action must appear in the saved binding scan. The hotkey path remains hardware-dependent because LMU may capture the keyboard trigger before the virtual press. Cancellation, errors, page navigation, and backend teardown stop setup, and the vJoy backend releases every output when it disconnects.

Each selected profile exposes a compact mapping summary in the Profiles detail pane. **Manage virtual mappings** in the profile actions menu opens a dedicated workspace with assigned/unassigned filters and token-based partial search in any word order, so a long shared action layout does not overwhelm the profile overview. Search covers action names, physical buttons, vJoy outputs, and device names. The editor presents the action first—for example **Pit Limiter**—and keeps **vJoy B1** as secondary diagnostic information. A user chooses the action and presses the desired physical control; LMUKit stores physical control -> Pit Limiter, while the shared layout supplies Pit Limiter -> B1. Users do not manually calculate or memorise button numbers. Live remapping is started, switched, or stopped from the profile pane. The Virtual Input page is reserved for shared vJoy setup and global routing status.

For an existing profile, **Reuse existing bindings** reads that profile's saved `direct input.json` and joins both sides by logical action name (case-insensitively): the old LMU binding supplies the physical USB device and button, while the shared layout supplies the virtual button LMU now expects. This is an explicit, reviewable migration and does not edit either the profile's JSON or LMU's live file. Existing manual virtual mappings win, axes and POVs are ignored, unknown actions are skipped, and a physical control assigned to several matching actions is reported for manual resolution.

The live DirectInput reader and the importer identify a controller using the USB VID/PID encoded in DirectInput's product GUID. That is stable across reconnects and matches LMU's device metadata, but it is not a unique rim identity: two simultaneously connected devices with the same VID/PID cannot yet be distinguished automatically. Profile selection remains explicit, which is especially important for rims presented through the same Simagic base.

Importing the layout is read-only and never rewrites LMU's file. LMUKit rejects ambiguous layouts, including one virtual button bound to several LMU actions or one action bound to several buttons on the virtual device, and asks the user to resolve them in LMU. Axis and POV bindings on the virtual device are ignored because Virtual Input Mode is button-only.

Steering, throttle, brake, clutch, and FFB remain connected directly between the physical hardware and LMU. Virtual Input Mode never reads, emits, transforms, or suppresses those axes and never participates in force-feedback delivery. It manages only programmable buttons, switches, encoder steps, and optional digital paddles.

LMU continues to see the physical controller. A physical control managed through LMUKit should not also have a direct LMU binding, otherwise one press may trigger both the direct action and the translated virtual action. LMUKit does not hide or suppress physical devices.

## Profile contents and selection

Virtual mappings are an optional extension to an existing profile, not a silent reinterpretation of its saved `direct input.json`. The user can explicitly import matching button bindings and review or adjust the result. A profile retains its Legacy JSON configuration while also carrying a separate Virtual Input mapping.

Profiles continue to use reusable wheel references and the existing class tags. Several profiles may exist for the same wheel and class, for example:

- GT Neo / HY / Race;
- GT Neo / HY / Alternate;
- GT Neo / GT3 / Race;
- FX Pro / GT3 / Race.

Each profile owns its physical button mappings, modifier combinations, and encoder modes. Wheel/class defaults are stored as separate references: at most one default profile for a given wheel and class, plus an optional wheel-level fallback. Changing a profile's wheel or class tags and deleting a profile clean up any references that are no longer valid. Layers remain a planned extension.

For example, a GT Neo HY profile may map encoder 1 to Front ARB while a GT Neo GT3 profile maps the same encoder to TC2. Both profiles still target the globally stable virtual buttons assigned to those logical actions.

## Hold modifiers

A profile may map a direct trigger and a modifier-plus-trigger combination independently. **Bind control** handles both: press and release one button for a normal binding, or keep the first button held and press another to use the first as a modifier. For example, button 5 may map to Pit Limiter while button 4 + button 5 maps to Recenter VR.

The mapping engine tracks every currently pressed physical button. For a given trigger, the satisfied mapping with the most held modifiers wins; equal-specificity matches use a stable physical-button ordering. Changing modifier state while a trigger is held reconciles the active virtual output immediately. Output ownership remains reference-counted, and profile changes, disconnects, errors, and shutdown still release all outputs. The current UI captures one modifier per combination; the persisted and engine models allow additional modifiers in a later editor revision.

## Encoder modes

The encoder editor automatically listens for an Increase detent and then a Decrease detent as digital buttons; the user turns the encoder once and confirms each step. A single-mode encoder needs no other control. Adding a second mode asks for a separate mode-cycle button; each of up to eight ordered modes assigns a searchable named LMU action to Increase and Decrease. Pressing the mode control advances once per physical press, emits no virtual button itself, and updates the on-screen active-mode indicator. Encoder controls are reserved for that encoder and cannot also be ordinary mappings or controls for another encoder.

Each captured detent queues a 32 ms virtual-button pulse followed by at least 8 ms released. Pulses are serialised in arrival order and bounded to prevent unrestrained backlog. Profile changes, routing shutdown, and backend errors clear pending work and release active outputs. A regular held mapping that targets the same virtual button retains ownership when an encoder pulse finishes.

## Daily use

1. Start LMUKit and LMU with the already configured virtual controller available.
2. LMUKit uses a reliably identified wheel or the user's manual wheel selection.
3. The user starts a profile manually from the Profiles detail pane, chooses a wheel and class on the Virtual Input page to resolve the exact class default and then the wheel fallback, or explicitly enables automatic startup for the most recently used live profile.
4. LMUKit translates physical button events through that profile into the stable virtual layout.
5. The user may change virtual profiles while LMU is running without rebinding the virtual controller.

The current wheel and class selection is manual. LMUKit shows whether the active route was selected directly, from a class default, or from a wheel fallback. Future automatic context detection must not override an active manual profile selection.

## Operating modes

- **Legacy JSON:** profile activation safely backs up and replaces LMU's live `direct input.json`; LMU must be restarted because it does not hot-reload the file.
- **Virtual Input:** profile activation changes the in-memory physical-to-logical mapping; LMU keeps the same virtual device and bindings.

Automatic live-profile startup is disabled by default. When enabled, it runs only if the persisted global mode is Virtual Input and a previously successful live profile still exists. A startup failure remains visible in virtual-input health and never falls back to Legacy JSON or rewrites LMU's configuration.

The modes share profile identity, wheel metadata, and class tags, but keep their binding data distinct. Importing an old JSON profile never silently creates a complete virtual mapping.

Virtual Input Mode remains experimental until the hardware gates and recovery behavior in the [feasibility record](virtual-input-feasibility.md) are complete.
