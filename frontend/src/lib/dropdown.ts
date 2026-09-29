export function positionDropdown(
  details: HTMLDetailsElement,
  preferredHeight: number,
) {
  if (!details.open) return;

  const trigger = details.getBoundingClientRect();
  const scrollViewport = details.closest<HTMLElement>(".content");
  const viewport = scrollViewport?.getBoundingClientRect();
  const top = Math.max(0, viewport?.top ?? 0);
  const bottom = Math.min(window.innerHeight, viewport?.bottom ?? window.innerHeight);
  const roomAbove = Math.max(0, trigger.top - top - 8);
  const roomBelow = Math.max(0, bottom - trigger.bottom - 8);
  const dropUp = roomBelow < preferredHeight && roomAbove > roomBelow;
  const available = dropUp ? roomAbove : roomBelow;

  details.classList.toggle("drop-up", dropUp);
  details.style.setProperty(
    "--dropdown-space",
    `${Math.min(preferredHeight, available)}px`,
  );
}
