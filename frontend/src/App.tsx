import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity,
  ChevronRight,
  CircleGauge,
  Gamepad2,
  Image as ImageIcon,
  Keyboard,
  LayoutDashboard,
  Minus,
  Search,
  Settings,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  Upload,
  Wrench,
  X,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type Profile = {
  id: string;
  name: string;
  hotkeySlot: number | null;
  bindingCount: number;
  devices: Device[];
  hasWheelImage: boolean;
};

type Device = {
  key: string;
  name: string;
  bindingCount: number;
};

type Snapshot = {
  profiles: Profile[];
  activeProfile: string | null;
  lmuConfigPath: string;
  lmuSettingsPath: string;
};

type Page = "home" | "profiles" | "bindings" | "game" | "settings";

const navItems = [
  ["home", "Overview", LayoutDashboard],
  ["profiles", "Profiles", Gamepad2],
  ["bindings", "Binding editor", SlidersHorizontal],
  ["game", "Game settings", Wrench],
  ["settings", "Settings", Settings],
] as const;

const emptySnapshot: Snapshot = {
  profiles: [],
  activeProfile: null,
  lmuConfigPath: "",
  lmuSettingsPath: "",
};

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("Ready");
  const [wheelImageUrl, setWheelImageUrl] = useState<string | null>(null);

  async function refresh() {
    try {
      const next = await invoke<Snapshot>("snapshot");
      setSnapshot(next);
      setSelectedId((current) => current ?? next.activeProfile ?? next.profiles[0]?.id ?? null);
    } catch (error) {
      setNotice(String(error));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  const selected =
    snapshot.profiles.find((profile) => profile.id === selectedId) ?? snapshot.profiles[0];
  const active = snapshot.profiles.find((profile) => profile.id === snapshot.activeProfile);
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return snapshot.profiles.filter((profile) => profile.name.toLowerCase().includes(needle));
  }, [query, snapshot.profiles]);

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;
    if (!selected?.hasWheelImage) {
      setWheelImageUrl(null);
      return;
    }
    void invoke<number[] | null>("profile_wheel_image", { profileId: selected.id })
      .then((bytes) => {
        if (!bytes || cancelled) return;
        const data = new Uint8Array(bytes);
        const type = data[0] === 0x89 ? "image/png" : data[0] === 0xff ? "image/jpeg" : "image/webp";
        objectUrl = URL.createObjectURL(new Blob([data], { type }));
        setWheelImageUrl(objectUrl);
      })
      .catch((error) => setNotice(String(error)));
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [selected?.id, selected?.hasWheelImage]);

  async function activate(profile: Profile) {
    setBusy(true);
    setNotice(`Preparing ${profile.name}…`);
    try {
      await invoke("activate_profile", { profileId: profile.id });
      await refresh();
      setNotice(`${profile.name} is ready for the next LMU launch.`);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function removeDevice(profile: Profile, device: Device) {
    setBusy(true);
    setNotice(`Removing ${device.name} from ${profile.name}…`);
    try {
      const removed = await invoke<number>("remove_profile_device", {
        profileId: profile.id,
        deviceKey: device.key,
      });
      await refresh();
      setNotice(`${device.name} and ${removed} binding${removed === 1 ? "" : "s"} removed. Activate the profile again to apply it to LMU.`);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function saveWheelImage(profile: Profile, file: File) {
    if (file.size > 8 * 1024 * 1024) {
      setNotice("Wheel images must be 8 MB or smaller.");
      return;
    }
    setBusy(true);
    setNotice(`Assigning a wheel image to ${profile.name}…`);
    try {
      const imageBytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      await invoke("save_profile_wheel_image", { profileId: profile.id, imageBytes });
      await refresh();
      setNotice(`Wheel image assigned to ${profile.name}.`);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function removeWheelImage(profile: Profile) {
    setBusy(true);
    try {
      await invoke("remove_profile_wheel_image", { profileId: profile.id });
      await refresh();
      setNotice(`Wheel image removed from ${profile.name}.`);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  function windowAction(action: () => Promise<void>) {
    void action().catch((error) => setNotice(`Window action failed: ${String(error)}`));
  }

  return (
    <div className="window-shell">
      <div className="titlebar" data-tauri-drag-region onMouseDown={(event) => {
        if (event.button === 0 && !(event.target as HTMLElement).closest("button")) {
          windowAction(() => getCurrentWindow().startDragging());
        }
      }} onDoubleClick={(event) => {
        if (!(event.target as HTMLElement).closest("button")) {
          windowAction(() => getCurrentWindow().toggleMaximize());
        }
      }}>
        <div className="titlebar-brand" data-tauri-drag-region>
          <img src="/app-icon.png" alt="" />
          <span data-tauri-drag-region>LMUKit</span>
        </div>
        <div className="window-controls">
          <button aria-label="Minimize" onClick={() => windowAction(() => getCurrentWindow().minimize())}><Minus /></button>
          <button aria-label="Maximize or restore" onClick={() => windowAction(() => getCurrentWindow().toggleMaximize())}><Square /></button>
          <button className="close" aria-label="Close" onClick={() => windowAction(() => getCurrentWindow().close())}><X /></button>
        </div>
      </div>
      <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><span>LMU</span></div>
          <div><strong>LMUKit</strong><small>Race setup, simplified</small></div>
        </div>

        <nav>
          <span className="nav-label">Workspace</span>
          {navItems.map(([id, label, Icon]) => (
            <button className={page === id ? "nav-item active" : "nav-item"} onClick={() => setPage(id)} key={id}>
              <Icon size={18} strokeWidth={1.8} />
              <span>{label}</span>
              {id === "profiles" && <em>{snapshot.profiles.length}</em>}
            </button>
          ))}
        </nav>

        <div className="sidebar-foot">
          <div className="game-state"><span className="pulse" /><div><strong>Workspace ready</strong><small>Close LMU before changes</small></div></div>
          <button className="keyboard-hint"><Keyboard size={16} /><span>Quick switcher</span><kbd>Ctrl Alt Space</kbd></button>
        </div>
      </aside>

      <main>
        <header className="topbar">
          <div><span className="eyebrow">{page === "home" ? "Good evening" : "LMUKit"}</span><h1>{pageTitle(page)}</h1></div>
          <div className="top-actions"><button className="ghost"><Search size={17} /> Search <kbd>⌘ K</kbd></button><button className="avatar">A</button></div>
        </header>

        <div className="content">
          {page === "home" && <Overview active={active} selected={selected} wheelImageUrl={wheelImageUrl} profiles={snapshot.profiles} onOpenProfiles={() => setPage("profiles")} onActivate={activate} busy={busy} />}
          {page === "profiles" && <Profiles profiles={filtered} selected={selected} wheelImageUrl={wheelImageUrl} activeId={snapshot.activeProfile} selectedId={selectedId} query={query} setQuery={setQuery} setSelectedId={setSelectedId} onActivate={activate} onRemoveDevice={removeDevice} onSaveWheelImage={saveWheelImage} onRemoveWheelImage={removeWheelImage} busy={busy} />}
          {page !== "home" && page !== "profiles" && <ComingSoon page={page} selected={selected} />}
        </div>

        <footer><span className="status-dot" />{notice}<span className="footer-rule" /><span>Changes are backed up automatically</span></footer>
      </main>
      </div>
    </div>
  );
}

function Overview({ active, selected, wheelImageUrl, profiles, onOpenProfiles, onActivate, busy }: { active?: Profile; selected?: Profile; wheelImageUrl: string | null; profiles: Profile[]; onOpenProfiles: () => void; onActivate: (profile: Profile) => void; busy: boolean }) {
  const hero = selected ?? active;
  return <div className="dashboard-grid">
    <section className="hero-card">
      <div className="hero-copy">
        <span className="chip"><span className="pulse" /> Current setup</span>
        <h2>{active?.name ?? "Choose your race setup"}</h2>
        <p>{active ? `${active.bindingCount} controls across ${Math.max(active.devices.length, 1)} connected device${active.devices.length === 1 ? "" : "s"}.` : "Select a profile to prepare LMU for your next session."}</p>
        <div className="hero-actions">
          {hero && <button className="primary" disabled={busy || hero.id === active?.id} onClick={() => onActivate(hero)}><Zap size={17} fill="currentColor" />{hero.id === active?.id ? "Prepared" : "Use this profile"}</button>}
          <button className="secondary" onClick={onOpenProfiles}>View profiles <ChevronRight size={17} /></button>
        </div>
      </div>
      <div className="wheel-stage"><div className="halo" /><img src={wheelImageUrl ?? "/wheel-hero.png"} alt={wheelImageUrl ? `Wheel assigned to ${hero?.name ?? "this profile"}` : "A generic GT racing wheel and wheelbase"} /></div>
    </section>

    <section className="metric-card"><div className="metric-icon teal"><Gamepad2 /></div><div><span>Saved profiles</span><strong>{profiles.length.toString().padStart(2, "0")}</strong><small>Ready for LMU</small></div></section>
    <section className="metric-card"><div className="metric-icon amber"><CircleGauge /></div><div><span>Active bindings</span><strong>{active?.bindingCount ?? 0}</strong><small>{active?.devices.length ?? 0} input devices</small></div></section>
    <section className="metric-card"><div className="metric-icon violet"><Activity /></div><div><span>Profile health</span><strong className="word">Synced</strong><small>No pending changes</small></div></section>

    <section className="panel recent">
      <div className="section-head"><div><span className="eyebrow">Your garage</span><h3>Race profiles</h3></div><button className="text-button" onClick={onOpenProfiles}>Manage all <ChevronRight size={16} /></button></div>
      <div className="profile-strip">
        {profiles.slice(0, 4).map((profile, index) => <button key={profile.id} className={profile.id === active?.id ? "mini-profile active" : "mini-profile"} onClick={() => onActivate(profile)}>
          <span className="profile-number">0{index + 1}</span><div><strong>{profile.name}</strong><small>{profile.bindingCount} bindings · {profile.devices[0]?.name ?? "Input profile"}</small></div>{profile.hotkeySlot && <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>}
        </button>)}
        {!profiles.length && <div className="empty"><Sparkles size={22} /><span>Your saved profiles will appear here.</span></div>}
      </div>
    </section>

    <section className="panel quick-actions"><div className="section-head"><div><span className="eyebrow">Get moving</span><h3>Quick actions</h3></div></div><button><Upload /><div><strong>Import preset</strong><small>Drop in an LMU JSON file</small></div><ChevronRight /></button><button><SlidersHorizontal /><div><strong>Edit bindings</strong><small>Map controls outside LMU</small></div><ChevronRight /></button><button><Wrench /><div><strong>Tune game settings</strong><small>FFB, display and more</small></div><ChevronRight /></button></section>
  </div>;
}

function Profiles({ profiles, selected, wheelImageUrl, activeId, selectedId, query, setQuery, setSelectedId, onActivate, onRemoveDevice, onSaveWheelImage, onRemoveWheelImage, busy }: { profiles: Profile[]; selected?: Profile; wheelImageUrl: string | null; activeId: string | null; selectedId: string | null; query: string; setQuery: (value: string) => void; setSelectedId: (value: string) => void; onActivate: (profile: Profile) => void; onRemoveDevice: (profile: Profile, device: Device) => void; onSaveWheelImage: (profile: Profile, file: File) => void; onRemoveWheelImage: (profile: Profile) => void; busy: boolean }) {
  const [confirmDevice, setConfirmDevice] = useState<string | null>(null);
  return <section className="profiles-page panel"><div className="profiles-toolbar"><div className="search-box"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search your profiles" /></div><button className="primary"><Upload size={17} /> Import preset</button></div><div className="profile-table-head"><span>Profile</span><span>Devices</span><span>Bindings</span><span>Shortcut</span><span /></div>{profiles.map((profile) => <div className={selectedId === profile.id ? "profile-row selected" : "profile-row"} key={profile.id} onClick={() => { setSelectedId(profile.id); setConfirmDevice(null); }}><div className="profile-main"><span className="device-thumb"><Gamepad2 /></span><div><strong>{profile.name}</strong><small>{profile.id === activeId ? "Prepared for next launch" : "Saved profile"}</small></div></div><span>{profile.devices.slice(0, 2).map((device) => device.name).join(", ") || "Unknown device"}</span><span>{profile.bindingCount}</span><span>{profile.hotkeySlot ? <kbd>Ctrl Alt {profile.hotkeySlot}</kbd> : <small>Not set</small>}</span><button className={profile.id === activeId ? "prepared" : "secondary compact"} disabled={busy || profile.id === activeId} onClick={(event) => { event.stopPropagation(); void onActivate(profile); }}>{profile.id === activeId ? "Prepared" : "Use profile"}</button></div>)}{!profiles.length && <div className="empty large"><Gamepad2 /><strong>No profiles found</strong><span>Import a preset or capture your current LMU bindings.</span></div>}{selected && <div className="device-manager"><div className="device-manager-head"><div><span className="eyebrow">Profile setup</span><h3>{selected.name}</h3></div><div className="profile-image-actions">{wheelImageUrl && <img src={wheelImageUrl} alt="Assigned wheel" />}<label className="secondary compact image-picker"><ImageIcon />{selected.hasWheelImage ? "Replace wheel image" : "Assign wheel image"}<input type="file" accept="image/png,image/jpeg,image/webp" disabled={busy} onChange={(event) => { const file = event.target.files?.[0]; if (file) void onSaveWheelImage(selected, file); event.target.value = ""; }} /></label>{selected.hasWheelImage && <button className="remove-device" disabled={busy} onClick={() => void onRemoveWheelImage(selected)}><Trash2 /> Remove image</button>}</div></div><p className="device-note">Removing a device also removes all of its bindings.</p><div className="device-list">{selected.devices.map((device) => <div className="device-row" key={device.key}><span className="device-thumb"><Gamepad2 /></span><div><strong>{device.name}</strong><small>{device.bindingCount} binding{device.bindingCount === 1 ? "" : "s"} · {device.key}</small></div>{confirmDevice === device.key ? <div className="confirm-remove"><span>Remove?</span><button className="danger" disabled={busy} onClick={() => void onRemoveDevice(selected, device)}>Yes, remove</button><button className="secondary compact" onClick={() => setConfirmDevice(null)}>Cancel</button></div> : <button className="remove-device" aria-label={`Remove ${device.name}`} onClick={() => setConfirmDevice(device.key)}><Trash2 /> Remove</button>}</div>)}{!selected.devices.length && <div className="empty">This profile has no registered devices.</div>}</div></div>}</section>;
}

function ComingSoon({ page, selected }: { page: Page; selected?: Profile }) {
  return <section className="focus-placeholder"><span className="chip">Interface migration</span><h2>{pageTitle(page)}</h2><p>The existing {pageTitle(page).toLowerCase()} functionality is being connected to this new workspace without changing your saved data.</p>{selected && <div className="selected-context"><Gamepad2 /><div><small>Current profile</small><strong>{selected.name}</strong></div></div>}</section>;
}

function pageTitle(page: Page) {
  return ({ home: "Your race workspace", profiles: "Profiles", bindings: "Binding editor", game: "Game settings", settings: "Settings" })[page];
}
