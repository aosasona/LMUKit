import {
  AlertTriangle,
  Cable,
  Check,
  ChevronRight,
  CircleHelp,
  FileJson,
  Gamepad2,
  Gauge,
  Layers3,
  Radio,
  RefreshCw,
  Settings,
  SlidersHorizontal,
  Tags,
  Usb,
} from "lucide-react";
import { useState } from "react";
import type { InputMode } from "../components/InputModeSwitch";
import type { Page } from "../models";

type GuideMode = "legacy" | "virtual";

type GuideStep = {
  title: string;
  detail: string;
  action?: string;
  page?: Page;
};

const virtualSteps: GuideStep[] = [
  {
    title: "Prepare vJoy once",
    detail:
      "Install vJoy separately, configure device 1 with 128 buttons, and restart Windows if its installer asks. LMUKit never installs or silently changes the driver.",
    action: "Check virtual input",
    page: "virtual",
  },
  {
    title: "Bind LMU actions to vJoy",
    detail:
      "Open LMU's control assignments, choose an action, then use LMUKit's guided setup to send the next unused virtual button. Finish and rescan after LMU saves the bindings.",
    action: "Open guided setup",
    page: "virtual",
  },
  {
    title: "Create or import a profile",
    detail:
      "Keep one profile for each control layout you use and give it the appropriate car-class tags. Wheel users can optionally associate reusable wheel details for presentation.",
    action: "Open profiles",
    page: "profiles",
  },
  {
    title: "Map and test physical controls",
    detail:
      "Choose a named LMU action, press the physical button, and confirm it. Hold one button and press another for a modifier. Stop live remapping before using Test a control.",
    action: "Choose a profile",
    page: "profiles",
  },
  {
    title: "Go live",
    detail:
      "Select Virtual Input in the title bar and use the profile live. Optionally set one default per class and enable Auto-select on Overview. Keep LMUKit running or hidden in the tray.",
    action: "Open overview",
    page: "home",
  },
];

const legacySteps: GuideStep[] = [
  {
    title: "Confirm LMU's file locations",
    detail:
      "LMUKit normally finds direct input.json and Settings.JSON automatically. Check them in Settings if LMU is installed in a non-standard Steam library.",
    action: "Open settings",
    page: "settings",
  },
  {
    title: "Import or capture a profile",
    detail:
      "Drag an existing DirectInput preset into Profiles, import it, or capture LMU's current configuration after creating the bindings you want in-game.",
    action: "Open profiles",
    page: "profiles",
  },
  {
    title: "Organise and edit",
    detail:
      "Assign reusable device information, add class tags, edit bindings and FFB values, and use the binding finder to understand physical controls before activating the profile.",
    action: "Open profiles",
    page: "profiles",
  },
  {
    title: "Prepare the next LMU launch",
    detail:
      "Close LMU, select Legacy JSON in the title bar, and activate the profile. LMU does not hot-reload direct input.json, so start or restart the game afterward.",
    action: "Open overview",
    page: "home",
  },
];

export function HelpPage({
  inputMode,
  onNavigate,
}: {
  inputMode: InputMode;
  onNavigate: (page: Page) => void;
}) {
  const [guideMode, setGuideMode] = useState<GuideMode>(inputMode);
  const steps = guideMode === "virtual" ? virtualSteps : legacySteps;

  return (
    <div className="help-workspace">
      <section className="help-hero">
        <div className="help-hero-icon">
          <CircleHelp />
        </div>
        <div>
          <span className="eyebrow">Start here</span>
          <h2>From physical control to LMU action</h2>
          <p>
            Set up a wheel, controller, button box, or mixed rig without
            memorising device numbers. Pick the workflow you want and follow it
            in order.
          </p>
        </div>
        <div className="help-current-mode">
          <span>Current input method</span>
          <strong>
            {inputMode === "virtual" ? "Virtual Input" : "Legacy JSON"}
          </strong>
        </div>
      </section>

      <section className="help-methods">
        <button
          type="button"
          className={guideMode === "virtual" ? "active" : ""}
          onClick={() => setGuideMode("virtual")}
        >
          <Radio />
          <span>
            <strong>Virtual Input</strong>
            <small>
              Switch button mappings live through one stable vJoy device.
            </small>
          </span>
          {guideMode === "virtual" && <Check />}
        </button>
        <button
          type="button"
          className={guideMode === "legacy" ? "active" : ""}
          onClick={() => setGuideMode("legacy")}
        >
          <FileJson />
          <span>
            <strong>Legacy JSON</strong>
            <small>
              Swap LMU's saved DirectInput configuration between launches.
            </small>
          </span>
          {guideMode === "legacy" && <Check />}
        </button>
      </section>

      <section className="help-section">
        <header>
          <div>
            <span className="eyebrow">
              {guideMode === "virtual"
                ? "Recommended live workflow"
                : "File-based workflow"}
            </span>
            <h3>
              {guideMode === "virtual"
                ? "Set up Virtual Input"
                : "Set up Legacy profiles"}
            </h3>
          </div>
          <span className="help-step-count">{steps.length} steps</span>
        </header>
        <div className="help-steps">
          {steps.map((step, index) => (
            <article key={step.title}>
              <span className="help-step-number">{index + 1}</span>
              <div>
                <h4>{step.title}</h4>
                <p>{step.detail}</p>
                {step.page && step.action && (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => onNavigate(step.page!)}
                  >
                    {step.action} <ChevronRight size={15} />
                  </button>
                )}
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="help-section">
        <header>
          <div>
            <span className="eyebrow">Hardware support</span>
            <h3>What you can connect</h3>
          </div>
        </header>
        <div className="device-support-grid">
          <article>
            <Gauge />
            <h4>Wheels and rims</h4>
            <p>
              Map programmable buttons, digital paddles, switches, modifiers,
              and encoders. Steering, pedals, and FFB stay directly connected to
              LMU.
            </p>
          </article>
          <article>
            <Gamepad2 />
            <h4>Controllers</h4>
            <p>
              Digital buttons work when the controller is visible to the
              DirectInput reader. Analog sticks, triggers, rumble, and
              XInput-only devices are not currently remapped.
            </p>
          </article>
          <article>
            <Layers3 />
            <h4>Button boxes and mixed rigs</h4>
            <p>
              A profile can map controls from several detected devices to the
              same stable LMU action layout. Multiple controls may target one
              action safely.
            </p>
          </article>
        </div>
        <div className="help-safety-note">
          <AlertTriangle />
          <div>
            <strong>Virtual Input is deliberately button-only</strong>
            <span>
              LMUKit does not suppress physical hardware, transform axes, or
              participate in force feedback. Avoid binding the same physical
              button directly in LMU and through vJoy, or both inputs may fire.
            </span>
          </div>
        </div>
      </section>

      <section className="help-section help-troubleshooting">
        <header>
          <div>
            <span className="eyebrow">Quick answers</span>
            <h3>Troubleshooting</h3>
          </div>
        </header>
        <div className="help-faq-grid">
          <details>
            <summary>
              <Usb /> LMUKit does not detect my button
            </summary>
            <p>
              Turn the device on before capture and try Bind control again. A
              controller must expose the button through DirectInput; D-pads
              reported only as POV hats and XInput-only controls may not appear.
            </p>
          </details>
          <details>
            <summary>
              <Cable /> LMU reports that vJoy disconnected
            </summary>
            <p>
              Open Configure vJoy and confirm device 1 still exists with the
              same button count. Changing its configuration can create a new
              device identity, requiring the virtual buttons to be bound in LMU
              again.
            </p>
          </details>
          <details>
            <summary>
              <RefreshCw /> The automatic profile is wrong
            </summary>
            <p>
              Check the profile's class tags and its global class default.
              Choose a profile manually to pause automation, or select Resume
              Auto on Overview after correcting the default.
            </p>
          </details>
          <details>
            <summary>
              <SlidersHorizontal /> A physical button triggers twice
            </summary>
            <p>
              Remove any direct LMU binding for that physical control when it is
              also routed through vJoy. Use Test a control with live remapping
              disabled to check which named action LMUKit resolves.
            </p>
          </details>
          <details>
            <summary>
              <Tags /> Why did my Legacy profile not change in-game?
            </summary>
            <p>
              LMU does not hot-reload direct input.json. Activate the profile
              while LMU is closed, then launch the game. Virtual Input is the
              mode intended for live button-layout changes.
            </p>
          </details>
          <details>
            <summary>
              <Settings /> Where are logs and diagnostics?
            </summary>
            <p>
              Open Settings and use its Diagnostics section to reveal logs or
              export a privacy-redacted report. Input events themselves are not
              written to the log.
            </p>
          </details>
        </div>
      </section>
    </div>
  );
}
