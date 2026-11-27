export const DEFAULT_LMU_CLASS_TAGS = [
  "GT3",
  "GTE",
  "LMP3",
  "LMP2",
  "HY",
] as const;

export function displayClassTag(tag: string): string {
  return tag.toUpperCase() === "GT3" ? "LMGT3" : tag;
}
