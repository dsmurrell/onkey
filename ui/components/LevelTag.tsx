import { levelAbout, levelName, levelOf } from "@/lib/music";
import { useT } from "@/lib/i18n";
export { bestAtLatestLevel } from "@/lib/music";

/** A small tag naming the level a score was taken at, for the ones not scored at Relaxed (the default): a Pro 70
 *  and a Relaxed 70 aren't the same score. Nothing for Relaxed, or for takes from before levels. */
export default function LevelTag({ level, className = "" }: { level?: string | null; className?: string }) {
  const t = useT();
  const l = levelOf(level);
  if (l === "relaxed") return null;
  return (
    <span title={t("levelTag.title", { level: levelName(l), about: levelAbout(l) })}
          className={`rounded-full bg-notice-fill/12 px-1.5 py-px text-[10px] font-medium text-notice ${className}`}>
      {levelName(l)}
    </span>
  );
}
