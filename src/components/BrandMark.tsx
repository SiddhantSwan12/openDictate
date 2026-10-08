/** OpenDictate's mark: a speech wave inside a rounded square. */
export function BrandMark({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <defs>
        <linearGradient id="od-g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6366f1" />
          <stop offset="1" stopColor="#a855f7" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="8" fill="url(#od-g)" />
      {[
        [8, 12, 8],
        [12.5, 8, 16],
        [17, 10, 12],
        [21.5, 6, 20],
        [26, 13, 6],
      ].map(([x, y, h], i) => (
        <rect key={i} x={x - 1.25} y={y} width="2.5" height={h} rx="1.25" fill="white" />
      ))}
    </svg>
  );
}
