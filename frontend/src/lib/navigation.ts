import type { Page } from "../models";

export const pageTitle = (page: Page) =>
  ({
    home: "Your race workspace",
    profiles: "Profiles",
    automatic: "Automatic selection",
    wheels: "Wheels",
    bindings: "Profile editor",
    virtual: "Virtual input",
    game: "Game settings",
    settings: "Settings",
    help: "Help & setup",
  })[page];
