import { Gamepad2 } from "lucide-react";
import type { Page, Profile } from "../models";

export const pageTitle = (page: Page) =>
  ({
    home: "Your race workspace",
    profiles: "Profiles",
    wheels: "Wheels",
    bindings: "Profile editor",
    game: "Game settings",
    settings: "Settings",
  })[page];

export function ComingSoonPage({
  page,
  selected,
}: {
  page: Page;
  selected?: Profile;
}) {
  return (
    <section className="focus-placeholder">
      <span className="chip">Interface migration</span>
      <h2>{pageTitle(page)}</h2>
      <p>
        The existing {pageTitle(page).toLowerCase()} functionality is being
        connected to this new workspace without changing your saved data.
      </p>
      {selected && (
        <div className="selected-context">
          <Gamepad2 />
          <div>
            <small>Current profile</small>
            <strong>{selected.name}</strong>
          </div>
        </div>
      )}
    </section>
  );
}
