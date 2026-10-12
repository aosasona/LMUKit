# Native OpenXR overlays: research and proposed direction

This document records a possible future LMUKit overlay system. It is a design
note, not an implemented or advertised feature. The first candidate widgets
are standings, a session weather forecast, and pedal traces.

## Product boundary

LMUKit should own the complete overlay path. It must not require
OpenKneeboard, a browser-overlay host, or another dashboard application. A
normal desktop process cannot independently submit composition layers to Le
Mans Ultimate's OpenXR session, so the proposed implementation is a native
OpenXR API layer loaded into LMU's OpenXR process.

The API layer is an OpenXR loader mechanism, not an ad-hoc function hook. It
would intercept the minimum necessary OpenXR lifecycle and frame-submission
calls, create an overlay swapchain, and append one or more
`XrCompositionLayerQuad` entries when forwarding LMU's `xrEndFrame` call. It
must use LMU's existing session rather than create a competing XR session.

```text
LMUKit desktop process
    -> LMU telemetry and REST readers
    -> bounded widget-specific processors
    -> immutable shared-memory snapshots
    -> native renderer / shared GPU texture
    -> LMUKit OpenXR API-layer DLL inside LMU
    -> OpenXR composition layer(s) submitted with LMU's frame
    -> active OpenXR runtime
```

The layer should be installed and enabled only with explicit consent. Prefer
an OpenXR manifest with a dedicated `enable_environment` condition so the DLL
loads only for an LMU process launched with LMUKit overlay support enabled. It
must have a documented disable/recovery path and must not silently affect
unrelated OpenXR applications.

## Feasibility gate

Do not build the dashboard widgets before a minimal Windows hardware spike
passes all of these checks:

1. Register and load an LMUKit API layer for an LMU OpenXR launch.
2. Record the actual graphics binding and runtime used by LMU; do not assume
   Direct3D 11, Direct3D 12, SteamVR, Meta, Pimax, or VDXR behavior.
3. Create one transparent swapchain and display a static quad.
4. Verify correct alpha, pose, scale, frame timing, shutdown, and recovery
   when the layer is disabled.
5. Measure CPU time, GPU time, allocations, and frame-time variance with and
   without the layer.
6. Verify that LMU and any active anti-cheat accept the API layer. Do not claim
   compatibility until this succeeds in the real game.
7. Repeat the smoke test on the OpenXR runtimes that LMUKit intends to support.

The API layer runs inside the game process, so a crash or stall can affect LMU.
Failure must be fail-open where possible: forwarding the game's original call
without an overlay is preferable to taking down the OpenXR session.

## Runtime and rendering boundaries

Keep telemetry parsing, REST requests, layout editing, persistence, and other
product logic in the normal LMUKit process. The in-process layer should have a
small native surface and perform no JSON parsing, filesystem scanning, React
rendering, WebView work, or blocking IPC on LMU's frame thread.

The proposed hot path should:

- reuse OpenXR swapchains and graphics resources;
- consume versioned immutable snapshots through non-blocking shared memory;
- allocate nothing while intercepting `xrEndFrame`;
- update a widget texture only when its data or presentation changes;
- reuse glyph, icon, geometry, and command resources;
- use premultiplied alpha correctly;
- cap work and retain the previous valid texture if a producer is late;
- append only as many composition layers as the runtime reports it can accept;
- expose timing and failure counters without logging telemetry samples.

Start with one composed transparent overlay texture or atlas. Independent
movable widget layers can follow after validating runtime layer limits and the
cost of additional swapchains. A purpose-built native renderer using the
session's Windows graphics API and sharp native text is preferable to running
a browser renderer inside LMU. React remains appropriate for the desktop
configuration and preview UI.

## Data and update budgets

The current `lmukit-context` 1.5-second car-context worker should remain
separate. Overlays require a coherent high-frequency telemetry reader that
copies the game-owned mapping once per source update and publishes only narrow
widget snapshots. The reader must use LMU's update counters or another
verified consistency contract so the UI never consumes a torn scoring frame.

Suggested initial budgets:

| Data | Source | Target processing rate |
| --- | --- | --- |
| Pedal controls | `LMU_Data` vehicle telemetry | up to 60 Hz |
| Standings | `LMU_Data` scoring | about 5 Hz |
| Current environment | `LMU_Data` scoring | about 1 Hz |
| Session forecast | `GET /rest/sessions/weather` | once per session/configuration change |

These are ceilings, not reasons to repaint every frame. Stop or reduce work
when the corresponding overlay is hidden.

## Initial widgets

### Standings

LMU shared memory appears to expose driver and vehicle names, class, overall
place, completed laps and progress, current/last/best times, gaps, pit state,
pit-stop count, penalties, finish state, and player identity. Confirm every
field against the installed official contract before adding it to
`lmukit-context`.

The default VR presentation should favour a stable, readable window around the
player with optional class leaders rather than rendering a large full field.
Avoid row movement animation and visually noisy decoration. Car numbers,
ratings, incidents, and team data must remain absent unless a verified source
provides them.

### Weather forecast

The reviewed iRDashies implementation reads `GET /rest/sessions/weather` and
expects `PRACTICE`, `QUALIFY`, and `RACE` forecasts with `START`, `NODE_25`,
`NODE_50`, `NODE_75`, and `FINISH` nodes. Each node supplies sky,
temperature, and rain-chance values. Validate this endpoint and its semantics
against the user's installed LMU build before treating it as a contract.

Reject incomplete forecasts instead of inventing missing nodes. A compact
five-stage horizontal timeline with a clearly selected session is preferable
to imitating the current-conditions card from iRDashies.

### Pedal traces

LMU shared memory appears to expose filtered and unfiltered throttle, brake,
clutch, and steering values, plus gear, speed, and ABS state. LMUKit should
remain read-only: this feature visualises axes but does not route, transform,
suppress, or emit them and does not participate in force feedback.

Use fixed-size typed ring buffers and a native line renderer. Make the history
duration configurable, distinguish throttle, brake, and clutch consistently,
and make steering optional. React state must not be updated for every sample.

## Design requirements

- Design for headset angular size and legibility rather than desktop pixels.
- Support head-locked and world-locked placement, distance, scale, opacity,
  visibility, and a deliberate recenter action.
- Prefer strong hierarchy, tabular numerals, stable columns, large type, and
  restrained class accents.
- Avoid blur, continuous decorative animation, dense chrome, and effects that
  consume fill rate without improving readability.
- Provide a desktop preview using the same layout model, without making the
  WebView the headset renderer.
- Keep overlay layout configuration separate from wheel/input profiles unless
  a later use case explicitly links them.

## References reviewed

- [iRDashies](https://github.com/tariknz/irdashies), including its LMU shared
  memory structures, processors, widget support table, input trace, and
  `/rest/sessions/weather` parser.
- [OpenXR loader design and API layers](https://registry.khronos.org/OpenXR/specs/1.1/loader.html).
- [OpenXR 1.1 composition and `xrEndFrame`](https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html).
- [LunarG OpenXR OverlayLayer](https://github.com/LunarG/OpenXR-OverlayLayer),
  an experimental Windows/D3D11 API-layer and remote-overlay proof of concept;
  it is prior art, not a production dependency decision.

