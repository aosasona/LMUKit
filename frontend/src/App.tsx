import { invoke } from "@tauri-apps/api/core";
import { Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import type { Binding, Device, Page, Profile, Snapshot, Wheel } from "./models";
import { emptySnapshot } from "./models";
import { BindingEditorPage } from "./pages/BindingEditorPage";
import { ComingSoonPage, pageTitle } from "./pages/ComingSoonPage";
import { OverviewPage } from "./pages/OverviewPage";
import { ProfilesPage } from "./pages/ProfilesPage";
import { SettingsPage } from "./pages/SettingsPage";

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("Ready");
  const [wheelImageUrl, setWheelImageUrl] = useState<string | null>(null);
  const [wheelImageRevision, setWheelImageRevision] = useState(0);
  const [bindings, setBindings] = useState<Binding[]>([]);
  const selected =
    snapshot.profiles.find((profile) => profile.id === selectedId) ??
    snapshot.profiles[0];
  const active = snapshot.profiles.find(
    (profile) => profile.id === snapshot.activeProfile,
  );
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return snapshot.profiles.filter((profile) =>
      [profile.name, ...profile.wheelTags, ...profile.classTags]
        .concat(
          profile.wheelBrand ?? "",
          profile.wheelName ?? "",
          ...profile.customTags,
        )
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  }, [query, snapshot.profiles]);

  async function refresh() {
    try {
      const next = await invoke<Snapshot>("snapshot");
      setSnapshot(next);
      setSelectedId((current) =>
        next.profiles.some((profile) => profile.id === current)
          ? current
          : (next.activeProfile ?? next.profiles[0]?.id ?? null),
      );
    } catch (error) {
      setNotice(String(error));
    }
  }
  useEffect(() => {
    void refresh();
  }, []);
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--font-scale",
      snapshot.uiFontScale.toString(),
    );
  }, [snapshot.uiFontScale]);
  useEffect(() => {
    let url: string | null = null;
    let cancelled = false;
    if (!selected?.hasWheelImage) {
      setWheelImageUrl(null);
      return;
    }
    void invoke<number[] | null>("profile_wheel_image", {
      profileId: selected.id,
    })
      .then((bytes) => {
        if (!bytes || cancelled) return;
        const data = new Uint8Array(bytes);
        const type =
          data[0] === 0x89
            ? "image/png"
            : data[0] === 0xff
              ? "image/jpeg"
              : "image/webp";
        url = URL.createObjectURL(new Blob([data], { type }));
        setWheelImageUrl(url);
      })
      .catch((error) => setNotice(String(error)));
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [selected?.id, selected?.hasWheelImage, wheelImageRevision]);

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
  useEffect(() => {
    if (page === "bindings") void loadBindings();
  }, [page, selected?.id]);
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
      setNotice(`Preparing ${profile.name}…`);
      await invoke("activate_profile", { profileId: profile.id });
      await refresh();
      setNotice(`${profile.name} is ready for the next LMU launch.`);
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
  const saveWheelImage = (profile: Profile, file: File) =>
    withBusy(async () => {
      if (file.size > 8 * 1024 * 1024)
        throw new Error("Wheel images must be 8 MB or smaller.");
      await invoke("save_profile_wheel_image", {
        profileId: profile.id,
        imageBytes: Array.from(new Uint8Array(await file.arrayBuffer())),
      });
      setWheelImageRevision((revision) => revision + 1);
      await refresh();
      setNotice(`Wheel image assigned to ${profile.name}.`);
    });
  const removeWheelImage = (profile: Profile) =>
    withBusy(async () => {
      await invoke("remove_profile_wheel_image", { profileId: profile.id });
      setWheelImageRevision((revision) => revision + 1);
      await refresh();
      setNotice(`Wheel image removed from ${profile.name}.`);
    });
  const saveCategories = (
    profile: Profile,
    classTags: string[],
    customTags: string[],
  ) =>
    withBusy(async () => {
      await invoke("set_profile_categories", {
        profileId: profile.id,
        classTags,
        customTags,
      });
      await refresh();
      setNotice(`Categories updated for ${profile.name}.`);
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
      await Promise.all([refresh(), loadBindings(profile)]);
      setNotice(
        `${binding.alternate ? "Alternate " : ""}${binding.action} binding cleared. A backup was created.`,
      );
    });
  const saveFontScale = (scale: number) =>
    withBusy(async () => {
      await invoke("set_ui_font_scale", { scale });
      setSnapshot((current) => ({ ...current, uiFontScale: scale }));
      setNotice(`Text size set to ${Math.round(scale * 100)}%.`);
    });

  return (
    <div className="window-shell">
      <TitleBar onError={setNotice} />
      <div className="app-shell">
        <Sidebar
          page={page}
          profileCount={snapshot.profiles.length}
          onNavigate={setPage}
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
              <button className="ghost">
                <Search size={17} /> Search <kbd>⌘ K</kbd>
              </button>
              <button className="avatar">A</button>
            </div>
          </header>
          <div className="content">
            {page === "home" && (
              <OverviewPage
                active={active}
                selected={selected}
                wheelImageUrl={wheelImageUrl}
                profiles={snapshot.profiles}
                busy={busy}
                onNavigate={setPage}
                onActivate={activate}
                onUpdate={updateProfile}
              />
            )}
            {page === "profiles" && (
              <ProfilesPage
                profiles={filtered}
                wheels={snapshot.wheels}
                selected={selected}
                wheelImageUrl={wheelImageUrl}
                activeId={snapshot.activeProfile}
                activeDirty={snapshot.activeProfileDirty}
                selectedId={selectedId}
                query={query}
                busy={busy}
                setQuery={setQuery}
                setSelectedId={setSelectedId}
                onActivate={activate}
                onImport={importPreset}
                onCapture={captureProfile}
                onUpdate={updateProfile}
                onDelete={deleteProfile}
                onReveal={revealProfiles}
                onRemoveDevice={removeDevice}
                onSaveWheelImage={saveWheelImage}
                onRemoveWheelImage={removeWheelImage}
                onSaveCategories={saveCategories}
                onCreateWheel={createWheel}
                onAssignWheel={assignWheel}
              />
            )}
            {page === "bindings" && (
              <BindingEditorPage
                profile={selected}
                bindings={bindings}
                busy={busy}
                onClear={clearBinding}
              />
            )}
            {page === "game" && (
              <ComingSoonPage page={page} selected={selected} />
            )}
            {page === "settings" && (
              <SettingsPage
                fontScale={snapshot.uiFontScale}
                busy={busy}
                onSaveFontScale={saveFontScale}
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
    </div>
  );
}
