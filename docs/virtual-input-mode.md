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

| Active profile mapping | Stable virtual layout | One-time LMU binding |
| --- | --- | --- |
| GT Neo button 5 -> Pit Limiter | Pit Limiter -> vJoy B1 | B1 -> Pit Limiter |
| FX Pro button 12 -> Pit Limiter | Pit Limiter -> vJoy B1 | B1 -> Pit Limiter |
| GT Neo encoder CW -> Brake Bias + | Brake Bias + -> vJoy B8 | B8 -> Brake Bias + |

Because the virtual layout is shared, `B1` continues to mean Pit Limiter when the user changes wheel, class, or profile. Profile switching changes only LMUKit's live translation; it does not recreate the virtual controller or rewrite the LMU binding.

## Initial setup

The user installs and explicitly configures vJoy, then binds its virtual buttons in LMU once. vJoy is an optional external dependency: LMUKit does not bundle, install, update, or silently configure it. Legacy JSON mode remains fully usable without vJoy.

Only actions that should be managed by LMUKit need virtual bindings. After LMU has saved those bindings, LMUKit reads `direct input.json` and builds the shared layout from the selected virtual device: LMU input ID 32 is vJoy B1, ID 33 is B2, and so on. The LMU action name becomes the stable logical action.

The guided setup will let the user leave LMU listening on an action and trigger LMUKit with a configurable global hotkey. LMUKit selects the lowest unused button within the connected virtual device's actual capacity, emits one bounded press/release, and advances only after the user continues. The hotkey itself requires real LMU validation because LMU must capture the virtual press rather than the keyboard trigger; an in-app trigger remains available if that is unreliable. Cancellation, errors, and shutdown always release the output.

The mapping editor presents the action first—for example **Pit Limiter**—and keeps **vJoy B1** as secondary diagnostic information. A user chooses the action and presses the desired physical control; LMUKit stores physical control -> Pit Limiter, while the shared layout supplies Pit Limiter -> B1. Users do not manually calculate or memorise button numbers.

Importing the layout is read-only and never rewrites LMU's file. LMUKit rejects ambiguous layouts, including one virtual button bound to several LMU actions or one action bound to several buttons on the virtual device, and asks the user to resolve them in LMU. Axis and POV bindings on the virtual device are ignored because Virtual Input Mode is button-only.

Steering, throttle, brake, clutch, and FFB remain connected directly between the physical hardware and LMU. Virtual Input Mode never reads, emits, transforms, or suppresses those axes and never participates in force-feedback delivery. It manages only programmable buttons, switches, encoder steps, and optional digital paddles.

LMU continues to see the physical controller. A physical control managed through LMUKit should not also have a direct LMU binding, otherwise one press may trigger both the direct action and the translated virtual action. LMUKit does not hide or suppress physical devices.

## Profile contents and selection

Virtual mappings are an optional extension to an existing profile, not an automatic reinterpretation of its saved `direct input.json`. Adding a virtual mapping must be explicit and reviewable. A profile may retain its Legacy JSON configuration while also carrying a separate Virtual Input mapping.

Profiles continue to use reusable wheel references and the existing class tags. Several profiles may exist for the same wheel and class, for example:

- GT Neo / HY / Race;
- GT Neo / HY / Alternate;
- GT Neo / GT3 / Race;
- FX Pro / GT3 / Race.

Each profile owns its physical button, modifier, layer, and encoder-mode mappings. Wheel/class defaults are separate references: at most one default profile for a given wheel and class, plus an optional wheel-level fallback. A manual profile override always takes precedence until the user clears it.

For example, a GT Neo HY profile may map encoder 1 to Front ARB while a GT Neo GT3 profile maps the same encoder to TC2. Both profiles still target the globally stable virtual buttons assigned to those logical actions.

## Daily use

1. Start LMUKit and LMU with the already configured virtual controller available.
2. LMUKit uses a reliably identified wheel or the user's manual wheel selection.
3. The user selects a profile manually, or confirmed class context resolves a configured wheel/class default.
4. LMUKit translates physical button events through that profile into the stable virtual layout.
5. The user may change virtual profiles while LMU is running without rebinding the virtual controller.

If wheel identity or car class is unknown, LMUKit does not guess. Manual wheel, class, and profile selection remains available. Automatic selection must not override an active manual profile override.

## Operating modes

- **Legacy JSON:** profile activation safely backs up and replaces LMU's live `direct input.json`; LMU must be restarted because it does not hot-reload the file.
- **Virtual Input:** profile activation changes the in-memory physical-to-logical mapping; LMU keeps the same virtual device and bindings.

The modes share profile identity, wheel metadata, and class tags, but keep their binding data distinct. Importing an old JSON profile never silently creates a complete virtual mapping.

Virtual Input Mode remains experimental until the hardware gates and recovery behavior in the [feasibility record](virtual-input-feasibility.md) are complete.
