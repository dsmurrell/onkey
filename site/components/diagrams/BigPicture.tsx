"use client";
import { Flow, Frame, Node } from "@/components/explain/Frame";
import { useClock } from "@/components/explain/motion";
import { Fit } from "@/components/explain/Fit";

/** The diagram's words, in the page's language (docs.diagrams.big; built by text.ts). `steps`: one caption per step. */
export type BigText = {
  label: string; all: string; steps: string[]; onMac: string;
  musicApp: string; musicAppSub: string; hears: string; hearsSub: string; learns: string; learnsSub: string;
  sing: string; singSub: string; score: string; scoreSub: string;
};

/** The whole journey: music app → OnKey hears → learns the song → you sing → score. `step` lights one stage. */
export function BigPicture({ step = 4, all = false, text }: { step?: number; all?: boolean; text: BigText }) {
  const t = useClock("big", 3);
  const on = (i: number) => all || i === step;
  const reached = (i: number) => all || i <= step;
  return (
    <Frame label={text.label} caption={all ? text.all : text.steps[step]}>
      <rect x="196" y="22" width="268" height="100" rx="16" fill="none" stroke="var(--line)" strokeDasharray="4 4" />
      <Fit max={170} x="452" y="38" textAnchor="end" fontSize={11} fill="var(--faint)">{text.onMac}</Fit>
      <Flow d="M126 80 H186" colour="var(--music)" t={t} on={reached(1)} />
      <Flow d="M296 80 H340" colour="var(--music)" t={t} on={reached(2)} />
      <Flow d="M402 108 C402 150 380 176 352 196" colour="var(--singer)" t={t} on={reached(3)} />
      <Flow d="M278 222 H206" colour="var(--you)" t={t} on={reached(4)} />
      <Node x={70} y={80} w={110} colour="var(--music)" title={text.musicApp} sub={text.musicAppSub} on={on(0)} />
      <Node x={241} y={80} w={98} colour="var(--music)" title={text.hears} sub={text.hearsSub} on={on(1)} />
      <Node x={402} y={80} w={112} colour="var(--singer)" title={text.learns} sub={text.learnsSub} on={on(2)} />
      <Node x={340} y={222} w={124} colour="var(--you)" title={text.sing} sub={text.singSub} on={on(3)} />
      <Node x={140} y={222} w={124} colour="var(--score)" title={text.score} sub={text.scoreSub} on={on(4)} />
    </Frame>
  );
}
