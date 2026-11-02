import Link from "next/link";

/** The app's logo: a violet tile with a white pitch wave, and the name. */
export function LogoMark({ size = 32 }: { size?: number }) {
  return (
    <span
      className="relative grid shrink-0 place-items-center rounded-xl bg-gradient-to-br from-[#9b8aff] to-[#6252e0] shadow-lg shadow-[#6252e0]/30"
      style={{ width: size, height: size }}
    >
      <svg width={size * 0.56} height={size * 0.56} viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true">
        <path d="M3 14c2.5 0 2.5-6 5-6s2.5 8 5 8 2.5-10 5-10 2 4 3 4" />
      </svg>
    </span>
  );
}

export function Logo({ href, label }: { href: string; label: string }) {
  return (
    <Link href={href} className="flex items-center gap-2.5 rounded-lg" aria-label={label}>
      <LogoMark />
      <span className="text-[15px] font-semibold tracking-tight text-fg">OnKey</span>
    </Link>
  );
}
