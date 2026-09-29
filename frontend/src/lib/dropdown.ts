import { useEffect, type RefObject } from "react";

export function useDismissableDetails(
  details: RefObject<HTMLDetailsElement | null>,
) {
  useEffect(() => {
    const dismissOnOutsidePress = (event: PointerEvent) => {
      const element = details.current;
      if (element?.open && !element.contains(event.target as Node)) {
        element.removeAttribute("open");
      }
    };
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") details.current?.removeAttribute("open");
    };

    document.addEventListener("pointerdown", dismissOnOutsidePress);
    document.addEventListener("keydown", dismissOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissOnOutsidePress);
      document.removeEventListener("keydown", dismissOnEscape);
    };
  }, [details]);
}

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
