import type { Page } from "../models";

export const pageTitle = (page: Page) =>
  ({
    home: "Your race workspace",
    profiles: "Profiles",
    wheels: "Wheels",
    bindings: "Profile editor",
    game: "Game settings",
    settings: "Settings",
  })[page];
