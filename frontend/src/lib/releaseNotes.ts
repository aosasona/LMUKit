import currentRelease from "../../../release-notes.json";

export type BundledReleaseNotes = {
  title: string;
  summary: string;
  highlights: string[];
};

export const releaseNotesFor = (version: string | null) =>
  version === currentRelease.version
    ? (currentRelease satisfies BundledReleaseNotes)
    : null;
