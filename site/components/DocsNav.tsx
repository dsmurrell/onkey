"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";

export const DOCS = [
  { href: "/docs/", title: "Getting started" },
  { href: "/docs/how-it-works/", title: "How OnKey works" },
  { href: "/docs/scoring/", title: "Scoring explained" },
  { href: "/docs/privacy/", title: "Privacy & legal" },
  { href: "/docs/faq/", title: "FAQ" },
  { href: "/docs/your-voice/", title: "Singing comfortably" },
  { href: "/docs/beyond-pitch/", title: "Beyond pitch" },
  { href: "/docs/troubleshooting/", title: "Troubleshooting" },
  { href: "/docs/credits/", title: "Credits" },
];

export function DocsNav() {
  const path = usePathname();
  const norm = (p: string) => (p.endsWith("/") ? p : p + "/");
  return (
    <nav aria-label="Docs" className="flex flex-wrap gap-1 border-b border-line pb-4 text-sm md:grid md:gap-0.5 md:border-0 md:pb-0">
      <p className="mb-2 hidden px-3 text-xs font-medium uppercase tracking-wider text-faint md:block">Docs</p>
      {DOCS.map((d) => {
        const on = norm(path) === d.href;
        return (
          <Link
            key={d.href}
            href={d.href}
            aria-current={on ? "page" : undefined}
            className={`rounded-lg border border-line px-3 py-2.5 transition md:border-0 md:py-1.5 ${on ? "bg-accent-wash font-medium text-fg" : "text-muted hover:bg-panel hover:text-fg"}`}
          >
            {d.title}
          </Link>
        );
      })}
    </nav>
  );
}
