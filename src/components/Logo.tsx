/** aide brand mark: a 2x2 module grid where the active module is highlighted. */
export function Logo({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="6.2" height="6.2" rx="1.8" fill="currentColor" opacity="0.5" />
      <rect x="8.8" y="1" width="6.2" height="6.2" rx="1.8" fill="currentColor" opacity="0.5" />
      <rect x="1" y="8.8" width="6.2" height="6.2" rx="1.8" fill="currentColor" opacity="0.5" />
      <rect x="8.8" y="8.8" width="6.2" height="6.2" rx="3.1" className="fill-primary" />
    </svg>
  )
}
