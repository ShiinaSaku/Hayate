export function Logo({ className = 'h-16 w-16' }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 100 100"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-label="Hayate logo"
    >
      <g
        fill="none"
        stroke="currentColor"
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M 25 20 L 65 50 L 25 80" strokeOpacity="0.2" />
        <path d="M 40 20 L 80 50 L 40 80" strokeOpacity="0.6" />
        <path d="M 55 20 L 95 50 L 55 80" />
        <line x1="10" y1="50" x2="35" y2="50" strokeWidth="3" strokeOpacity="0.3" />
        <line x1="5" y1="35" x2="20" y2="35" strokeWidth="2" strokeOpacity="0.15" />
        <line x1="5" y1="65" x2="20" y2="65" strokeWidth="2" strokeOpacity="0.15" />
      </g>
    </svg>
  );
}
