import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Search, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { ConfirmDialog, type Confirmation } from "./components/ConfirmDialog";
import { InputModeSwitch, type InputMode } from "./components/InputModeSwitch";
import { TitleBar } from "./components/TitleBar";
import { QuickProfileSwitcher } from "./components/QuickProfileSwitcher";
import type {
  Binding,
  BindingAssignmentCandidate,
  AutoProfileSelectionStatus,
  CompanionApp,
  Device,
  JsonObject,
  LmuCarContextStatus,
  Page,
  Profile,
  Snapshot,
  VirtualRoutingStatus,
  Wheel,
} from "./models";
import {
  emptyAutoProfileSelectionStatus,
  emptyCarContextStatus,
  emptySnapshot,
} from "./models";
import { pageTitle } from "./lib/navigation";
import { GameSettingsPage } from "./pages/GameSettingsPage";
import { OverviewPage } from "./pages/OverviewPage";
import { ProfilesPage } from "./pages/ProfilesPage";
import { ProfileEditorPage } from "./pages/ProfileEditorPage";
import { SettingsPage } from "./pages/SettingsPage";
import { VirtualInputPage } from "./pages/VirtualInputPage";
import { WheelsPage } from "./pages/WheelsPage";

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot);
  const [routing, setRouting] = useState<VirtualRoutingStatus>({
    active: false,
    profileId: null,
    profileName: null,
    identity: null,
    routedEventCount: 0,
    lastEvent: null,
    error: null,
    selectionReason: null,
    activeEncoderModes: [],
  });
  const [carContext, setCarContext] = useState<LmuCarContextStatus>(
    emptyCarContextStatus,
  );
  const [autoProfileSelection, setAutoProfileSelection] =
    useState<AutoProfileSelectionStatus>(emptyAutoProfileSelectionStatus);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [notice, setNotice] = useState("Ready");
  const [wheelImageUrl, setWheelImageUrl] = useState<string | null>(null);
  const wheelImageObjectUrl = useRef<string | null>(null);
  const lastLiveProfileId = useRef<string | null>(null);
  const [wheelImageRevision, setWheelImageRevision] = useState(0);
  const [bindings, setBindings] = useState<Binding[]>([]);
  const [profileDocument, setProfileDocument] = useState<JsonObject | null>(
    null,
  );
  const [profileDocumentBaseline, setProfileDocumentBaseline] =
    useState<JsonObject | null>(null);
  const [profileDocumentLoading, setProfileDocumentLoading] = useState(false);
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const [exitRequested, setExitRequested] = useState(false);
  const [gameSettings, setGameSettings] = useState<JsonObject | null>(null);
  const [gameSettingsBaseline, setGameSettingsBaseline] =
    useState<JsonObject | null>(null);
  const [gameSettingsLoading, setGameSettingsLoading] = useState(false);
  const [dismissedDirtyProfileId, setDismissedDirtyProfileId] = useState<
    string | null
  >(null);
  const selected =
    snapshot.profiles.find((profile) => profile.id === selectedId) ??
    snapshot.profiles[0];
  const active = snapshot.profiles.find(
    (profile) => profile.id === snapshot.activeProfile,
  );
  const overviewProfile =
    snapshot.inputMode === "virtual"
      ? (selected ?? active)
      : (active ?? selected);
  const visibleActiveDirty =
    snapshot.inputMode === "legacy" &&
    snapshot.activeProfileDirty === true &&
    active?.id !== dismissedDirtyProfileId;
  const gameSettingsDirty = Boolean(
    gameSettings &&
      gameSettingsBaseline &&
      JSON.stringify(gameSettings) !== JSON.stringify(gameSettingsBaseline),
  );
  const profileDocumentDirty = Boolean(
    profileDocument &&
      profileDocumentBaseline &&
      JSON.stringify(profileDocument) !==
        JSON.stringify(profileDocumentBaseline),
  );
  const navigate = (next: Page) => {
    if (
      page === "game" &&
      next !== "game" &&
      gameSettingsDirty &&
      !window.confirm("Discard your unsaved game-setting changes?")
    )
      return;
    if (
      page === "bindings" &&
      next !== "bindings" &&
      profileDocumentDirty &&
      !window.confirm("Discard your unsaved profile changes?")
    )
      return;
    setPage(next);
  };
  async function refresh() {
    try {
      const [next, nextRouting, nextCarContext, nextAutoSelection] =
        await Promise.all([
          invoke<Snapshot>("snapshot"),
          invoke<VirtualRoutingStatus>("virtual_routing_status"),
          invoke<LmuCarContextStatus>("lmu_car_context_status"),
          invoke<AutoProfileSelectionStatus>("auto_profile_selection_status"),
        ]);
      setSnapshot(next);
      setRouting(nextRouting);
      setCarContext(nextCarContext);
      setAutoProfileSelection(nextAutoSelection);
      setSelectedId((current) =>
        next.profiles.some((profile) => profile.id === current)
          ? current
          : next.inputMode === "virtual" && nextRouting.active
            ? (nextRouting.profileId ?? next.profiles[0]?.id ?? null)
            : (next.activeProfile ?? next.profiles[0]?.id ?? null),
      );
    } catch (error) {
      setNotice(String(error));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    void refresh();
  }, []);
  useEffect(() => {
    const timer = window.setInterval(() => {
      void Promise.all([
        invoke<LmuCarContextStatus>("lmu_car_context_status"),
        invoke<AutoProfileSelectionStatus>("auto_profile_selection_status"),
      ])
        .then(([context, autoSelection]) => {
          setCarContext(context);
          setAutoProfileSelection(autoSelection);
        })
        .catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    const timer = window.setInterval(() => {
      void invoke<VirtualRoutingStatus>("virtual_routing_status")
        .then(setRouting)
        .catch(() => undefined);
    }, 500);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    const nextLiveProfileId = routing.active ? routing.profileId : null;
    if (nextLiveProfileId && nextLiveProfileId !== lastLiveProfileId.current) {
      setSelectedId(nextLiveProfileId);
    }
    lastLiveProfileId.current = nextLiveProfileId;
  }, [routing.active, routing.profileId]);
  useEffect(() => {
    const switcher = listen("open-profile-switcher", () => {
      setSwitcherOpen(true);
    });
    const activation = listen<{ Ok?: string; Err?: string }>(
      "profile-shortcut-result",
      (event) => {
        if (event.payload.Ok) {
          setNotice(event.payload.Ok);
          void refresh();
        } else if (event.payload.Err) {
          setNotice(event.payload.Err);
        }
      },
    );
    const virtualAutoStart = listen<string>(
      "virtual-autostart-result",
      (event) => {
        setNotice(event.payload);
        void invoke<VirtualRoutingStatus>("virtual_routing_status")
          .then((next) => {
            setRouting(next);
            if (next.active && next.profileId) setSelectedId(next.profileId);
          })
          .catch(() => undefined);
      },
    );
    const exitRequest = listen("request-exit-confirmation", () => {
      setExitRequested(true);
    });
    return () => {
      void switcher.then((unlisten) => unlisten());
      void activation.then((unlisten) => unlisten());
      void virtualAutoStart.then((unlisten) => unlisten());
      void exitRequest.then((unlisten) => unlisten());
    };
  }, []);
  useEffect(() => {
    const openSearch = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSwitcherOpen(true);
      }
    };
    window.addEventListener("keydown", openSearch);
    return () => window.removeEventListener("keydown", openSearch);
  }, []);
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--font-scale",
      snapshot.uiFontScale.toString(),
    );
  }, [snapshot.uiFontScale]);
  useEffect(() => {
    if (!snapshot.activeProfileDirty) setDismissedDirtyProfileId(null);
  }, [snapshot.activeProfileDirty]);
  useEffect(() => setDismissedDirtyProfileId(null), [snapshot.activeProfile]);
  useEffect(() => {
    let pendingUrl: string | null = null;
    let cancelled = false;
    if (!overviewProfile?.hasWheelImage) {
      const previousUrl = wheelImageObjectUrl.current;
      wheelImageObjectUrl.current = null;
      setWheelImageUrl(null);
      if (previousUrl) URL.revokeObjectURL(previousUrl);
      return;
    }
    void invoke<number[] | null>("profile_wheel_image", {
      profileId: overviewProfile.id,
    })
      .then(async (bytes) => {
        if (cancelled) return;
        if (!bytes) {
          const previousUrl = wheelImageObjectUrl.current;
          wheelImageObjectUrl.current = null;
          setWheelImageUrl(null);
          if (previousUrl) URL.revokeObjectURL(previousUrl);
          return;
        }
        const data = new Uint8Array(bytes);
        const type =
          data[0] === 0x89
            ? "image/png"
            : data[0] === 0xff
              ? "image/jpeg"
              : "image/webp";
        pendingUrl = URL.createObjectURL(new Blob([data], { type }));
        const image = new Image();
        image.src = pendingUrl;
        await image.decode();
        if (cancelled) return;

        const nextUrl = pendingUrl;
        const previousUrl = wheelImageObjectUrl.current;
        pendingUrl = null;
        wheelImageObjectUrl.current = nextUrl;
        setWheelImageUrl(nextUrl);
        if (previousUrl) URL.revokeObjectURL(previousUrl);
      })
      .catch((error) => {
        if (pendingUrl) {
          URL.revokeObjectURL(pendingUrl);
          pendingUrl = null;
        }
        if (!cancelled) setNotice(String(error));
      });
    return () => {
      cancelled = true;
      if (pendingUrl) {
        URL.revokeObjectURL(pendingUrl);
        pendingUrl = null;
      }
    };
  }, [overviewProfile?.id, overviewProfile?.hasWheelImage, wheelImageRevision]);
  useEffect(
    () => () => {
      if (wheelImageObjectUrl.current) {
        URL.revokeObjectURL(wheelImageObjectUrl.current);
      }
    },
    [],
  );

  async function loadBindings(profile = selected) {
    if (!profile) {
      setBindings([]);
      return;
    }
    try {
      setBindings(
        await invoke<Binding[]>("profile_bindings", { profileId: profile.id }),
      );
    } catch (error) {
      setNotice(String(error));
    }
  }
  async function loadProfileDocument(profile = selected) {
    if (!profile) {
      setProfileDocument(null);
      setProfileDocumentBaseline(null);
      return;
    }
    setProfileDocumentLoading(true);
    try {
      const document = await invoke<JsonObject>("load_profile_document", {
        profileId: profile.id,
      });
      setProfileDocument(document);
      setProfileDocumentBaseline(structuredClone(document));
    } catch (error) {
      setProfileDocument(null);
      setProfileDocumentBaseline(null);
      setNotice(String(error));
    } finally {
      setProfileDocumentLoading(false);
    }
  }
  useEffect(() => {
    if (page === "bindings") {
      void loadBindings();
      void loadProfileDocument();
    }
  }, [page, selected?.id]);
  async function loadGameSettings() {
    setGameSettingsLoading(true);
    try {
      const document = await invoke<JsonObject>("load_game_settings");
      setGameSettings(document);
      setGameSettingsBaseline(structuredClone(document));
    } catch (error) {
      setGameSettings(null);
      setGameSettingsBaseline(null);
      setNotice(String(error));
    } finally {
      setGameSettingsLoading(false);
    }
  }
  useEffect(() => {
    if (page === "game" && !gameSettings && !gameSettingsLoading)
      void loadGameSettings();
  }, [page]);
  async function withBusy(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  const activate = (profile: Profile) =>
    withBusy(async () => {
      setSelectedId(profile.id);
      if (snapshot.inputMode === "virtual") {
        setNotice(`Starting live remapping for ${profile.name}…`);
        setRouting(
          await invoke<VirtualRoutingStatus>("start_virtual_routing", {
            profileId: profile.id,
          }),
        );
        setNotice(`${profile.name} is now remapping live.`);
      } else {
        setNotice(`Preparing ${profile.name}…`);
        await invoke("activate_profile", { profileId: profile.id });
        await refresh();
        setNotice(`${profile.name} is ready for the next LMU launch.`);
      }
    });
  const changeInputMode = (inputMode: InputMode) => {
    if (inputMode === snapshot.inputMode) return;
    void withBusy(async () => {
      await invoke("set_input_mode", { inputMode });
      setSnapshot((current) => ({ ...current, inputMode }));
      if (inputMode === "legacy") {
        setRouting(
          await invoke<VirtualRoutingStatus>("virtual_routing_status"),
        );
      }
      setNotice(
        inputMode === "virtual"
          ? "Virtual Input selected. Choose a mapped profile to start live remapping."
          : "Legacy JSON selected. Live remapping stopped and all virtual buttons were released.",
      );
    });
  };
  const stopLiveRouting = () =>
    withBusy(async () => {
      setRouting(await invoke<VirtualRoutingStatus>("stop_virtual_routing"));
      setNotice("Live remapping disabled and all virtual buttons released.");
    });
  const changeAutoProfileSelection = (enabled: boolean) =>
    withBusy(async () => {
      setAutoProfileSelection(
        await invoke<AutoProfileSelectionStatus>("set_auto_profile_selection", {
          enabled,
        }),
      );
      setNotice(
        enabled
          ? "Automatic profile selection enabled. Waiting for LMU to confirm a class."
          : "Automatic profile selection disabled. The current live profile is unchanged.",
      );
    });
  const resumeAutoProfileSelection = () =>
    withBusy(async () => {
      setAutoProfileSelection(
        await invoke<AutoProfileSelectionStatus>(
          "resume_auto_profile_selection",
        ),
      );
      setNotice("Automatic profile selection resumed.");
    });
  const removeDevice = (profile: Profile, device: Device) =>
    withBusy(async () => {
      const removed = await invoke<number>("remove_profile_device", {
        profileId: profile.id,
        deviceKey: device.key,
      });
      await refresh();
      setNotice(
        `${device.name} and ${removed} binding${removed === 1 ? "" : "s"} removed. Activate the profile again to apply it to LMU.`,
      );
    });
  const saveCategories = (
    profile: Profile,
    profileName: string,
    classTags: string[],
    customTags: string[],
  ) =>
    withBusy(async () => {
      if (profileName.trim() !== profile.name) {
        await invoke("rename_profile", {
          profileId: profile.id,
          name: profileName,
        });
      }
      await invoke("set_profile_categories", {
        profileId: profile.id,
        classTags,
        customTags,
      });
      await refresh();
      setNotice(`${profileName.trim()} details updated.`);
    });
  const createWheel = (profile: Profile, brand: string, name: string) =>
    withBusy(async () => {
      const wheelId = await invoke<string>("create_wheel", {
        brand: brand || null,
        name,
      });
      await invoke("assign_profile_wheel", {
        profileId: profile.id,
        wheelId,
      });
      await refresh();
      setNotice(
        `${[brand, name].filter(Boolean).join(" ")} added to your wheel library.`,
      );
    });
  const assignWheel = (profile: Profile, wheel: Wheel | null) =>
    withBusy(async () => {
      await invoke("assign_profile_wheel", {
        profileId: profile.id,
        wheelId: wheel?.id ?? null,
      });
      await refresh();
      setNotice(
        wheel
          ? `${wheel.name} assigned to ${profile.name}.`
          : `Wheel removed from ${profile.name}.`,
      );
    });
  const createLibraryWheel = (brand: string, name: string) =>
    withBusy(async () => {
      await invoke("create_wheel", { brand: brand || null, name });
      await refresh();
      setNotice(
        `${[brand, name].filter(Boolean).join(" ")} added to your wheel library.`,
      );
    });
  const updateLibraryWheel = (wheel: Wheel, brand: string, name: string) =>
    withBusy(async () => {
      await invoke("update_wheel", {
        wheelId: wheel.id,
        brand: brand || null,
        name,
      });
      await refresh();
      setNotice(`${name.trim()} updated.`);
    });
  const deleteLibraryWheel = (wheel: Wheel) =>
    withBusy(async () => {
      await invoke("delete_wheel", { wheelId: wheel.id });
      await refresh();
      setNotice(`${wheel.name} deleted from the wheel library.`);
    });
  const saveLibraryWheelImage = (wheel: Wheel, file: File) =>
    withBusy(async () => {
      if (file.size > 8 * 1024 * 1024)
        throw new Error("Wheel images must be 8 MB or smaller.");
      await invoke("save_wheel_library_image", {
        wheelId: wheel.id,
        imageBytes: Array.from(new Uint8Array(await file.arrayBuffer())),
      });
      setWheelImageRevision((revision) => revision + 1);
      await refresh();
      setNotice(`${wheel.name} image updated for every linked profile.`);
    });
  const removeLibraryWheelImage = (wheel: Wheel) =>
    withBusy(async () => {
      await invoke("remove_wheel_library_image", { wheelId: wheel.id });
      setWheelImageRevision((revision) => revision + 1);
      await refresh();
      setNotice(`${wheel.name} image removed.`);
    });
  const importPreset = (file: File) =>
    withBusy(async () => {
      setNotice(`Importing ${file.name}…`);
      await invoke("import_profile", {
        name: file.name,
        contents: Array.from(new Uint8Array(await file.arrayBuffer())),
      });
      await refresh();
      setNotice(`${file.name} imported.`);
    });
  const captureProfile = (name: string) =>
    withBusy(async () => {
      await invoke("capture_profile", { name });
      await refresh();
      setNotice(`${name.trim()} captured from LMU.`);
    });
  const updateProfile = (profile: Profile) =>
    withBusy(async () => {
      await invoke("update_profile", { profileId: profile.id });
      await refresh();
      setNotice(
        `${profile.name} updated from LMU. The previous version was backed up.`,
      );
    });
  const deleteProfile = (profile: Profile) =>
    withBusy(async () => {
      await invoke("delete_profile", { profileId: profile.id });
      await refresh();
      setNotice(`${profile.name} deleted.`);
    });
  const revealProfiles = async () => {
    try {
      await invoke("reveal_profiles");
      setNotice("Opened the managed profiles folder.");
    } catch (error) {
      setNotice(String(error));
    }
  };
  const clearBinding = (profile: Profile, binding: Binding) =>
    withBusy(async () => {
      await invoke("clear_profile_binding", {
        profileId: profile.id,
        action: binding.action,
        alternate: binding.alternate,
      });
      await Promise.all([
        refresh(),
        loadBindings(profile),
        loadProfileDocument(profile),
      ]);
      setNotice(
        `${binding.alternate ? "Alternate " : ""}${binding.action} binding cleared. A backup was created.`,
      );
    });
  const assignBinding = (
    profile: Profile,
    action: string,
    alternate: boolean,
    candidate: BindingAssignmentCandidate,
  ) =>
    withBusy(async () => {
      await invoke("assign_profile_binding", {
        profileId: profile.id,
        action,
        alternate,
        deviceKey: candidate.deviceKey,
        inputId: candidate.inputId,
      });
      await Promise.all([
        refresh(),
        loadBindings(profile),
        loadProfileDocument(profile),
      ]);
      setNotice(
        `${alternate ? "Alternate " : ""}${action} assigned to ${candidate.control}. A backup was created.`,
      );
    });
  const saveFontScale = (scale: number) =>
    withBusy(async () => {
      await invoke("set_ui_font_scale", { scale });
      setSnapshot((current) => ({ ...current, uiFontScale: scale }));
      setNotice(`Text size set to ${Math.round(scale * 100)}%.`);
    });
  const setVirtualAutoStart = (enabled: boolean) =>
    withBusy(async () => {
      await invoke("set_virtual_auto_start", { enabled });
      setSnapshot((current) => ({
        ...current,
        autoStartVirtualProfile: enabled,
      }));
      setNotice(
        enabled
          ? "The last live profile will start automatically with LMUKit."
          : "Automatic live-profile startup disabled.",
      );
    });
  const saveAppSettings = (
    configPath: string,
    settingsPath: string,
    companionApps: CompanionApp[],
    launchMode: "desktop" | "vr",
  ) =>
    (async () => {
      try {
        await invoke("save_app_settings", {
          lmuConfigPath: configPath,
          lmuSettingsPath: settingsPath,
          companionApps,
          launchMode,
        });
        setSnapshot((current) => ({
          ...current,
          lmuConfigPath: configPath.trim(),
          lmuSettingsPath: settingsPath.trim(),
          companionApps,
          lmuLaunchMode: launchMode,
        }));
        setGameSettings(null);
        setGameSettingsBaseline(null);
        setNotice("Settings saved automatically.");
      } catch (error) {
        setNotice(String(error));
        throw error;
      }
    })();
  const setProfileHotkey = (profile: Profile, slot: number | null) =>
    withBusy(async () => {
      await invoke("set_profile_hotkey", { profileId: profile.id, slot });
      await refresh();
      setNotice(
        slot
          ? `${profile.name} assigned to Ctrl Alt ${slot}.`
          : `${profile.name} shortcut removed.`,
      );
    });
  const createLaunchOption = async (
    configPath: string,
    settingsPath: string,
    companionApps: CompanionApp[],
    launchMode: "desktop" | "vr",
  ) => {
    try {
      await invoke("save_app_settings", {
        lmuConfigPath: configPath,
        lmuSettingsPath: settingsPath,
        companionApps,
        launchMode,
      });
      const option = await invoke<string>("companion_launch_option");
      await refresh();
      setNotice(
        "Steam launch option copied. Paste it into LMU's Launch Options.",
      );
      return option;
    } catch (error) {
      setNotice(String(error));
      return null;
    }
  };
  const browseForPath = async (
    kind: "bindings" | "settings" | "executable",
  ) => {
    try {
      return await invoke<string | null>("pick_file", { kind });
    } catch (error) {
      setNotice(String(error));
      return null;
    }
  };
  const saveGameSettings = () =>
    withBusy(async () => {
      if (!gameSettings) return;
      await invoke("save_game_settings", { document: gameSettings });
      setGameSettingsBaseline(structuredClone(gameSettings));
      setNotice("Settings.JSON saved. A recovery backup was created.");
    });
  const saveProfileDocument = (activate: boolean) =>
    withBusy(async () => {
      if (!selected || !profileDocument) return;
      await invoke("save_profile_document", {
        profileId: selected.id,
        document: profileDocument,
        activate,
      });
      setProfileDocumentBaseline(structuredClone(profileDocument));
      await refresh();
      setNotice(
        activate
          ? `${selected.name} saved and prepared for the next LMU launch.`
          : `${selected.name} saved. A recovery backup was created.`,
      );
    });
  const exitConfirmation: Confirmation | null = exitRequested
    ? {
        title: "Exit LMUKit?",
        description: [
          routing.active
            ? "Live remapping will stop and every virtual button will be released."
            : "LMUKit will stop running in the notification area.",
          gameSettingsDirty || profileDocumentDirty
            ? "Your unsaved editor changes will be discarded."
            : "",
        ]
          .filter(Boolean)
          .join(" "),
        confirmLabel: "Exit LMUKit",
        danger: true,
        onConfirm: () => {
          setExitRequested(false);
          void invoke("exit_app").catch((error) => setNotice(String(error)));
        },
      }
    : null;

  return (
    <div className="window-shell">
      <TitleBar onError={setNotice} />
      <div className="app-shell">
        <Sidebar
          page={page}
          profileCount={snapshot.profiles.length}
          onNavigate={navigate}
          onExit={() => setExitRequested(true)}
        />
        <main>
          <header className="topbar">
            <div>
              <span className="eyebrow">
                {page === "home" ? "Good evening" : "LMUKit"}
              </span>
              <h1>{pageTitle(page)}</h1>
            </div>
            <div className="top-actions">
              <InputModeSwitch
                mode={snapshot.inputMode}
                busy={busy}
                onChange={changeInputMode}
              />
              {snapshot.inputMode === "virtual" && routing.active && (
                <button
                  type="button"
                  className="ghost live-routing-stop"
                  disabled={busy}
                  onClick={() => void stopLiveRouting()}
                >
                  <Square size={15} /> Disable remapping
                </button>
              )}
              <button
                type="button"
                className="ghost"
                onClick={() => setSwitcherOpen(true)}
              >
                <Search size={17} /> Search <kbd>Ctrl K</kbd>
              </button>
              <button className="avatar">A</button>
            </div>
          </header>
          <div className="content">
            {page === "home" && (
              <OverviewPage
                active={active}
                activeDirty={visibleActiveDirty}
                selected={selected}
                wheelImageUrl={wheelImageUrl}
                wheelImageRevision={wheelImageRevision}
                profiles={snapshot.profiles}
                busy={busy}
                inputMode={snapshot.inputMode}
                routing={routing}
                carContext={carContext}
                autoProfileSelection={autoProfileSelection}
                onSetAutoProfileSelection={changeAutoProfileSelection}
                onResumeAutoProfileSelection={resumeAutoProfileSelection}
                onDismissDirty={() =>
                  setDismissedDirtyProfileId(active?.id ?? null)
                }
                onNavigate={navigate}
                onActivate={activate}
                onUpdate={updateProfile}
              />
            )}
            {page === "profiles" && (
              <ProfilesPage
                profiles={snapshot.profiles}
                wheels={snapshot.wheels}
                selected={selected}
                wheelImageRevision={wheelImageRevision}
                activeId={snapshot.activeProfile}
                activeDirty={visibleActiveDirty}
                selectedId={selectedId}
                query={query}
                busy={busy}
                inputMode={snapshot.inputMode}
                routing={routing}
                onDismissDirty={() =>
                  setDismissedDirtyProfileId(active?.id ?? null)
                }
                loading={loading}
                setQuery={setQuery}
                setSelectedId={setSelectedId}
                onActivate={activate}
                onEdit={(profile) => {
                  setSelectedId(profile.id);
                  navigate("bindings");
                }}
                onImport={importPreset}
                onCapture={captureProfile}
                onUpdate={updateProfile}
                onDelete={deleteProfile}
                onReveal={revealProfiles}
                onRemoveDevice={removeDevice}
                onSaveCategories={saveCategories}
                onCreateWheel={createWheel}
                onAssignWheel={assignWheel}
                onNotice={setNotice}
              />
            )}
            {page === "bindings" && (
              <ProfileEditorPage
                profile={selected}
                bindings={bindings}
                document={profileDocument}
                baseline={profileDocumentBaseline}
                loading={profileDocumentLoading}
                busy={busy}
                wheelImageRevision={wheelImageRevision}
                onClear={clearBinding}
                onAssign={assignBinding}
                onChange={setProfileDocument}
                onSave={saveProfileDocument}
              />
            )}
            {page === "wheels" && (
              <WheelsPage
                wheels={snapshot.wheels}
                profiles={snapshot.profiles}
                busy={busy}
                imageRevision={wheelImageRevision}
                onCreate={createLibraryWheel}
                onUpdate={updateLibraryWheel}
                onDelete={deleteLibraryWheel}
                onSaveImage={saveLibraryWheelImage}
                onRemoveImage={removeLibraryWheelImage}
              />
            )}
            {page === "virtual" && (
              <VirtualInputPage
                busy={busy}
                inputMode={snapshot.inputMode}
                profiles={snapshot.profiles}
                onNotice={setNotice}
                onOpenProfiles={() => navigate("profiles")}
              />
            )}
            {page === "game" && (
              <GameSettingsPage
                document={gameSettings}
                baseline={gameSettingsBaseline}
                loading={gameSettingsLoading}
                busy={busy}
                onChange={setGameSettings}
                onReload={() => void loadGameSettings()}
                onSave={saveGameSettings}
              />
            )}
            {page === "settings" && (
              <SettingsPage
                fontScale={snapshot.uiFontScale}
                busy={busy}
                profiles={snapshot.profiles}
                configPath={snapshot.lmuConfigPath}
                settingsPath={snapshot.lmuSettingsPath}
                companionApps={snapshot.companionApps}
                launchMode={snapshot.lmuLaunchMode}
                autoStartVirtualProfile={snapshot.autoStartVirtualProfile}
                onSaveFontScale={saveFontScale}
                onSetVirtualAutoStart={setVirtualAutoStart}
                onSaveSettings={saveAppSettings}
                onSetHotkey={setProfileHotkey}
                onCreateLaunchOption={createLaunchOption}
                onBrowse={browseForPath}
              />
            )}
          </div>
          <footer>
            <span className="status-dot" />
            {notice}
            <span className="footer-rule" />
            <span>Changes are backed up automatically</span>
          </footer>
        </main>
      </div>
      {switcherOpen && (
        <QuickProfileSwitcher
          profiles={snapshot.profiles}
          activeId={
            snapshot.inputMode === "virtual"
              ? routing.profileId
              : snapshot.activeProfile
          }
          inputMode={snapshot.inputMode}
          busy={busy}
          imageRevision={wheelImageRevision}
          onClose={() => setSwitcherOpen(false)}
          onActivate={activate}
        />
      )}
      <ConfirmDialog
        confirmation={exitConfirmation}
        busy={busy}
        onClose={() => setExitRequested(false)}
      />
    </div>
  );
}
