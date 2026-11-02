"use client";
import { Flow, Frame } from "@/components/explain/Frame";
import { useClock } from "@/components/explain/motion";
import { Fit } from "@/components/explain/Fit";

/** The diagram's words, in the page's language (docs.diagrams.privacy; built by text.ts). `kept`: the four
 *  boxes on the Mac. `audio`, `lrclib`, `itunes`, `shazam`: a line per "\n" (SVG text doesn't wrap). */
export type PrivacyText = {
  label: string; all: string; steps: string[]; yourMac: string; kept: string[];
  audio: string; lrclib: string; itunes: string; shazam: string;
};

/** A box's height for its number of lines of text. */
const HEIGHTS = [0, 52, 62, 84];
const boxHeight = (lines: number) => HEIGHTS[lines] ?? 84 + 15 * (lines - 3);

/** A box outside the Mac: what it is, and what's sent to it (one line each). */
function Service({ y, title, lines, colour, on, dashed }: {
  y: number; title: string; lines: string[]; colour: string; on: boolean; dashed?: boolean;
}) {
  return (
    <g className="hl" opacity={on ? 1 : 0.35}>
      <rect x="318" y={y} width="148" height={boxHeight(lines.length)} rx="12" fill="var(--panel)" stroke={colour} strokeDasharray={dashed ? "5 3" : undefined} />
      <Fit max={136} x="392" y={y + 20} textAnchor="middle" fontSize={12.5} fontWeight="600" fill="var(--fg)">{title}</Fit>
      {lines.map((l, i) => (
        <Fit key={l} max={136} x="392" y={y + 37 + i * 15} textAnchor="middle" fontSize={10.5} fill="var(--muted)">{l}</Fit>
      ))}
    </g>
  );
}

/** What stays on the Mac, what's never kept, and the lookups that leave it. */
export function Privacy({ step = 3, all = false, text }: { step?: number; all?: boolean; text: PrivacyText }) {
  const t = useClock("privacy", 3);
  const on = (i: number) => all || i === step;
  const audio = text.audio.split("\n");
  return (
    <Frame label={text.label} caption={all ? text.all : text.steps[step]}>
      <rect x="20" y="28" width="250" height="236" rx="16" fill="var(--panel)" stroke="var(--line)" />
      <Fit max={218} x="36" y="50" fontSize={12} fontWeight="600" fill="var(--fg)">{text.yourMac}</Fit>
      <g className="hl" opacity={on(0) ? 1 : 0.35}>
        {text.kept.map((s, i) => (
          <g key={s} transform={`translate(32 ${66 + i * 30})`}>
            <rect width="226" height="24" rx="7" fill="var(--bg-raised)" stroke={i === 3 ? "var(--you)" : "var(--singer)"} strokeOpacity="0.7" />
            <Fit max={208} x="9" y="16" fontSize={11.5} fill="var(--fg-soft)">{s}</Fit>
          </g>
        ))}
      </g>
      <g className="hl" opacity={on(1) ? 1 : 0.35} transform="translate(32 196)">
        <rect width="226" height={12 + 16 * audio.length} rx="8" fill="none" stroke="var(--off)" strokeDasharray="4 3" />
        {audio.map((l, i) => (
          <Fit key={l} max={208} x="9" y={18 + 16 * i} fontSize={11.5} fill="var(--fg-soft)">{l}</Fit>
        ))}
      </g>
      <Flow d="M270 61 H318" colour="var(--singer)" t={t} on={on(2)} />
      <Flow d="M270 128 H318" colour="var(--singer)" t={t} on={on(2)} />
      <Flow d="M270 208 H318" colour="var(--music)" t={t} on={on(3)} />
      <Service y={30} title="LRCLIB" lines={text.lrclib.split("\n")} colour="var(--singer)" on={on(2)} />
      <Service y={102} title="iTunes Search" lines={text.itunes.split("\n")} colour="var(--singer)" on={on(2)} />
      <Service y={166} title="Shazam" lines={text.shazam.split("\n")} colour="var(--music)" on={on(3)} dashed />
    </Frame>
  );
}
