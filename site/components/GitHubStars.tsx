"use client";
import { useEffect, useState } from "react";
import { GITHUB_URL } from "@/lib/site";
import { starsLabel } from "@/lib/format";

let cached: number | null | undefined;

/** The repo's star count, fetched from GitHub's API in the browser. Shows nothing until it's known, or if GitHub
 *  doesn't answer (the repo is private, or the hourly limit for this visitor is used up). */
/** `label`: the page's `common.stars` message as written (an ICU plural on {n}), formatted here in `locale`. */
export function GitHubStars({ label, locale }: { label: string; locale: string }) {
  const [n, setN] = useState<number | null>(cached ?? null);
  useEffect(() => {
    if (cached !== undefined) return;
    const repo = GITHUB_URL.replace(/^https:\/\/github\.com\//, "").replace(/\/$/, "");
    fetch(`https://api.github.com/repos/${repo}`)
      .then(r => (r.ok ? r.json() : null))
      .then(j => { const v = typeof j?.stargazers_count === "number" ? j.stargazers_count : null; cached = v; setN(v); })
      .catch(() => { cached = null; });
  }, []);
  if (n == null || n < 1) return null;
  const shown = n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1).replace(/\.0$/, "")}k` : String(n);
  return (
    <span className="ml-1 inline-flex items-center gap-1 rounded-md border border-line px-1.5 text-sm tabular text-muted" aria-label={starsLabel(locale, label, n)}>
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" fill="currentColor"><path d="M8 .25l2.06 4.17 4.6.67-3.33 3.25.79 4.58L8 10.76l-4.12 2.16.79-4.58L1.34 5.09l4.6-.67z" /></svg>
      {shown}
    </span>
  );
}
