"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";

/** `items`: the pages in the page's language (app/…/docs/layout: a translated page's link stays in it). */
export function DocsNav({ items, label, heading }: { items: { href: string; title: string }[]; label: string; heading: string }) {
  const path = usePathname();
  const norm = (p: string) => (p.endsWith("/") ? p : p + "/");
  return (
    <nav aria-label={label} className="flex flex-wrap gap-1 border-b border-line pb-4 text-sm md:grid md:gap-0.5 md:border-0 md:pb-0">
      <p className="mb-2 hidden px-3 text-xs font-medium uppercase tracking-wider text-faint md:block">{heading}</p>
      {items.map((d) => {
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
