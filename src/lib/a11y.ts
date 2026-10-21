/**
 * Keyboard activation helpers for non-native clickable elements (cards, rows,
 * list items). Keeps the Enter/Space handling consistent across components.
 */
export function isActivationKey(key: string): boolean {
  return key === "Enter" || key === " " || key === "Spacebar"
}
