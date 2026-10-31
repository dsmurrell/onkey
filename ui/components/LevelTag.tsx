import { LEVELS, levelOf } from "@/lib/music";
export { bestAtLatestLevel } from "@/lib/music";

/** A small tag naming the level a score was taken at, for the ones not scored at Relaxed (the default): a Pro 70
 *  and a Relaxed 70 aren't the same score. Nothing for Relaxed, or for takes from before levels. */
export default function LevelTag({ level, className = "" }: { level?: string | null; className?: string }) {
  const l = levelOf(level);
  if (l === "relaxed") return null;
  return (
    <span title={`Scored at ${LEVELS[l].name}: ${LEVELS[l].about}`}
          className={`rounded-full bg-amber-400/12 px-1.5 py-px text-[10px] font-medium text-amber-200 ${className}`}>
      {LEVELS[l].name}
    </span>
  );
}
