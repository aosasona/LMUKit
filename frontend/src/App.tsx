import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  ChevronRight,
  CircleGauge,
  Gamepad2,
  Keyboard,
  LayoutDashboard,
  Search,
  Settings,
  SlidersHorizontal,
  Sparkles,
  Upload,
  Wrench,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type Profile = {
  id: string;
  name: string;
  hotkeySlot: number | null;
  bindingCount: number;
  devices: string[];
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

  return (
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
          {page === "home" && <Overview active={active} selected={selected} profiles={snapshot.profiles} onOpenProfiles={() => setPage("profiles")} onActivate={activate} busy={busy} />}
          {page === "profiles" && <Profiles profiles={filtered} activeId={snapshot.activeProfile} selectedId={selectedId} query={query} setQuery={setQuery} setSelectedId={setSelectedId} onActivate={activate} busy={busy} />}
          {page !== "home" && page !== "profiles" && <ComingSoon page={page} selected={selected} />}
        </div>

        <footer><span className="status-dot" />{notice}<span className="footer-rule" /><span>Changes are backed up automatically</span></footer>
      </main>
    </div>
  );
}

function Overview({ active, selected, profiles, onOpenProfiles, onActivate, busy }: { active?: Profile; selected?: Profile; profiles: Profile[]; onOpenProfiles: () => void; onActivate: (profile: Profile) => void; busy: boolean }) {
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
      <div className="wheel-stage"><div className="halo" /><img src="/wheel-hero.png" alt="A generic GT racing wheel and wheelbase" /></div>
    </section>

    <section className="metric-card"><div className="metric-icon teal"><Gamepad2 /></div><div><span>Saved profiles</span><strong>{profiles.length.toString().padStart(2, "0")}</strong><small>Ready for LMU</small></div></section>
    <section className="metric-card"><div className="metric-icon amber"><CircleGauge /></div><div><span>Active bindings</span><strong>{active?.bindingCount ?? 0}</strong><small>{active?.devices.length ?? 0} input devices</small></div></section>
    <section className="metric-card"><div className="metric-icon violet"><Activity /></div><div><span>Profile health</span><strong className="word">Synced</strong><small>No pending changes</small></div></section>

    <section className="panel recent">
      <div className="section-head"><div><span className="eyebrow">Your garage</span><h3>Race profiles</h3></div><button className="text-button" onClick={onOpenProfiles}>Manage all <ChevronRight size={16} /></button></div>
      <div className="profile-strip">
        {profiles.slice(0, 4).map((profile, index) => <button key={profile.id} className={profile.id === active?.id ? "mini-profile active" : "mini-profile"} onClick={() => onActivate(profile)}>
          <span className="profile-number">0{index + 1}</span><div><strong>{profile.name}</strong><small>{profile.bindingCount} bindings · {profile.devices[0] ?? "Input profile"}</small></div>{profile.hotkeySlot && <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>}
        </button>)}
        {!profiles.length && <div className="empty"><Sparkles size={22} /><span>Your saved profiles will appear here.</span></div>}
      </div>
    </section>

    <section className="panel quick-actions"><div className="section-head"><div><span className="eyebrow">Get moving</span><h3>Quick actions</h3></div></div><button><Upload /><div><strong>Import preset</strong><small>Drop in an LMU JSON file</small></div><ChevronRight /></button><button><SlidersHorizontal /><div><strong>Edit bindings</strong><small>Map controls outside LMU</small></div><ChevronRight /></button><button><Wrench /><div><strong>Tune game settings</strong><small>FFB, display and more</small></div><ChevronRight /></button></section>
  </div>;
}

function Profiles({ profiles, activeId, selectedId, query, setQuery, setSelectedId, onActivate, busy }: { profiles: Profile[]; activeId: string | null; selectedId: string | null; query: string; setQuery: (value: string) => void; setSelectedId: (value: string) => void; onActivate: (profile: Profile) => void; busy: boolean }) {
  return <section className="profiles-page panel"><div className="profiles-toolbar"><div className="search-box"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search your profiles" /></div><button className="primary"><Upload size={17} /> Import preset</button></div><div className="profile-table-head"><span>Profile</span><span>Devices</span><span>Bindings</span><span>Shortcut</span><span /></div>{profiles.map((profile) => <div className={selectedId === profile.id ? "profile-row selected" : "profile-row"} key={profile.id} onClick={() => setSelectedId(profile.id)}><div className="profile-main"><span className="device-thumb"><Gamepad2 /></span><div><strong>{profile.name}</strong><small>{profile.id === activeId ? "Prepared for next launch" : "Saved profile"}</small></div></div><span>{profile.devices.slice(0, 2).join(", ") || "Unknown device"}</span><span>{profile.bindingCount}</span><span>{profile.hotkeySlot ? <kbd>Ctrl Alt {profile.hotkeySlot}</kbd> : <small>Not set</small>}</span><button className={profile.id === activeId ? "prepared" : "secondary compact"} disabled={busy || profile.id === activeId} onClick={(event) => { event.stopPropagation(); void onActivate(profile); }}>{profile.id === activeId ? "Prepared" : "Use profile"}</button></div>)}{!profiles.length && <div className="empty large"><Gamepad2 /><strong>No profiles found</strong><span>Import a preset or capture your current LMU bindings.</span></div>}</section>;
}

function ComingSoon({ page, selected }: { page: Page; selected?: Profile }) {
  return <section className="focus-placeholder"><span className="chip">Interface migration</span><h2>{pageTitle(page)}</h2><p>The existing {pageTitle(page).toLowerCase()} functionality is being connected to this new workspace without changing your saved data.</p>{selected && <div className="selected-context"><Gamepad2 /><div><small>Current profile</small><strong>{selected.name}</strong></div></div>}</section>;
}

function pageTitle(page: Page) {
  return ({ home: "Your race workspace", profiles: "Profiles", bindings: "Binding editor", game: "Game settings", settings: "Settings" })[page];
}
