"use client";
import { Frame, wavePath } from "@/components/explain/Frame";
import { ease, useClock } from "@/components/explain/motion";
import { DUR, PHRASE, singerPitch, waveform } from "./song";

/** The diagram's words, in the page's language (docs.diagrams.learning; built by text.ts). `words`: the phrase's
 *  syllables, one per note of PHRASE. */
export type LearningText = {
  label: string; steps: string[]; singer: string; everythingElse: string; mix: string; timing: string; words: string[];
};

const MIX = waveform(160, "mix", 5), VOC = waveform(160, "vocal", 7), BACK = waveform(160, "backing", 11);
const X0 = 34, XW = 412;
const x = (t: number) => X0 + (t / DUR) * XW;
const ROW = 17, BASE = 222;
const y = (n: number) => BASE - (n + 0.5) * ROW;

/** Learning a song, step by step: separate → pitch → notes → lyrics → syllable bars. */
export function Learning({ step = 5, text }: { step?: number; text: LearningText }) {
  const t = useClock(`learn-${step}`, 3);
  const k = ease(t, 0.1, 1.3);                       // progress of this step's animation
  const pitchPts: string[] = [];
  const upto = step === 2 ? k * DUR : DUR;
  let pen = false;
  for (let s = 0; s <= upto; s += 0.02) {
    const p = singerPitch(s);
    if (Number.isNaN(p)) { pen = false; continue; }
    pitchPts.push(`${pen ? "L" : "M"}${x(s).toFixed(1)},${y(p).toFixed(1)}`);
    pen = true;
  }
  const split = step === 1 ? k : step > 1 ? 1 : 0;
  const lane = step >= 2;

  return (
    <Frame label={text.label} caption={text.steps[step]}>
      {/* the lane's rows, once there's a melody to put on it */}
      {lane && Array.from({ length: 10 }, (_, i) => (
        <rect key={i} x={X0} y={BASE - (i + 1) * ROW} width={XW} height={ROW} fill={i % 2 ? "var(--panel)" : "transparent"} opacity="0.6" />
      ))}
      {/* step 0–1: the mix, then it splits into the singer (up) and the backing (down) */}
      {!lane && (
        <>
          <path d={wavePath(MIX, X0, 150, XW, 34)} fill="none" stroke="var(--music)" strokeWidth="1.2" opacity={1 - split} />
          <g opacity={split}>
            <path d={wavePath(VOC, X0, 150 - 62 * split, XW, 30)} fill="none" stroke="var(--singer)" strokeWidth="1.3" />
            <path d={wavePath(BACK, X0, 150 + 62 * split, XW, 24)} fill="none" stroke="var(--music)" strokeWidth="1.1" opacity="0.7" />
            <text x={X0} y={150 - 62 * split - 38} fontSize="11.5" fill="var(--singer)">{text.singer}</text>
            <text x={X0} y={150 + 62 * split + 38} fontSize="11.5" fill="var(--music)">{text.everythingElse}</text>
          </g>
          {step === 0 && <text x={X0} y="104" fontSize="11.5" fill="var(--music)">{text.mix}</text>}
        </>
      )}
      {/* step 2+: the singer's pitch line */}
      {lane && <path d={pitchPts.join(" ")} fill="none" stroke="var(--singer)" strokeWidth={step === 2 ? 2.2 : 1.4} strokeOpacity={step === 2 ? 1 : 0.45} strokeLinecap="round" />}
      {/* step 3+: note bars */}
      {step >= 3 && PHRASE.map(([s, e, n], i) => {
        const g = step === 3 ? ease(t, 0.1 + i * 0.12, 0.4) : 1;
        return <rect key={i} x={x(s)} y={y(n) - ROW * 0.36} width={(x(e) - x(s)) * g} height={ROW * 0.72} rx={ROW * 0.36} fill="var(--singer)" fillOpacity={step >= 5 ? 0.95 : 0.75} />;
      })}
      {/* step 4+: words, first at the lyric file's times (a little off), then where they're heard */}
      {step >= 4 && PHRASE.map(([s, e, n], i) => {
        const settle = step === 4 ? ease(t, 0.4, 1.2) : 1;
        const off = 0.38 * (1 - settle);
        return (
          <text key={i} x={x(s + off) + (x(e) - x(s)) / 2} y={step >= 5 ? y(n) + 22 : 250} textAnchor="middle" fontSize="11.5"
            fill={step >= 5 ? "var(--fg-soft)" : "var(--singer)"} opacity={step === 4 ? 0.5 + 0.5 * settle : 1}>{text.words[i]}</text>
        );
      })}
      {step === 4 && <text x={X0 + XW} y="268" textAnchor="end" fontSize="10.5" fill="var(--faint)">{text.timing}</text>}
    </Frame>
  );
}
