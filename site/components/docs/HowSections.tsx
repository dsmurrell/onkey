import { Levels } from "@/components/explain/Levels";
import { Scrolly } from "@/components/explain/Scrolly";
import { Glyph } from "@/components/explain/Glyph";
import { DATA_DIR, codeUrl } from "@/lib/site";
import { getT, pageLink } from "@/lib/i18n";
import { diagramText } from "@/components/diagrams/text";
import { BigPicture } from "@/components/diagrams/BigPicture";
import { Hearing } from "@/components/diagrams/Hearing";
import { Learning } from "@/components/diagrams/Learning";
import { Placement } from "@/components/diagrams/Placement";
import { Echo } from "@/components/diagrams/Echo";
import { CoveragePlayground, CreditPlayground, ScoreDiagram } from "@/components/diagrams/Scoring";
import { Privacy } from "@/components/diagrams/Privacy";

// The seven sections of "How OnKey works", their words from docs.howItWorks in the catalog (the diagrams' own
// words from docs.diagrams, through diagramText). A server component: the diagrams and the Scrolly run in the
// browser and get their words as props.

/** A link to a source file, for "The exact numbers". */
function Code({ path }: { path: string }) {
  return <a href={codeUrl(path)}><code>{path.split("/").pop()}</code></a>;
}

/** "The exact numbers": the facts, with where they live in the code. */
function Hood({ children }: { children: React.ReactNode }) {
  return <ul className="!mt-0 max-w-[65ch]">{children}</ul>;
}

const S = ({ title, children }: { title: string; children: React.ReactNode }) => (
  <><p className="!mt-0 text-sm font-medium uppercase tracking-wider text-faint">{title}</p><p>{children}</p></>
);

type Tag = (chunks: React.ReactNode) => React.ReactNode;

/** The small animated pictures a step may put next to a term: an empty tag in the message (<wave></wave>) marks
 *  where the picture goes. */
const GLYPHS: Record<string, Tag> = Object.fromEntries(
  (["separate", "lane", "wave", "fingerprint", "echo", "score", "mic", "lock"] as const).map((kind) => [kind, () => <Glyph kind={kind} />]),
);

const SRC = "src-tauri/src/";
/** A source file as a tag (<dsp>dsp.rs</dsp>): the link shows the file's own name, whatever the message says. */
const file = (path: string): Tag => () => <Code path={path} />;
const src = (name: string) => file(`${SRC}${name}`);

export function HowSections({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.howItWorks");
  const d = diagramText(locale);
  const hoodTitle = t("exactNumbers");
  const scrolly = { locale, stepOf: d.stepOf };
  return (
    <>
      <Levels
        id="big-picture" title={t("big.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.big.label} diagram={<BigPicture text={d.big} />} steps={[
            <S key={0} title={d.big.steps[0]}>{rich("big.steps.play", GLYPHS)}</S>,
            <S key={1} title={d.big.steps[1]}>{rich("big.steps.hears", GLYPHS)}</S>,
            <S key={2} title={d.big.steps[2]}>{rich("big.steps.learns", GLYPHS)}</S>,
            <S key={3} title={d.big.steps[3]}>{rich("big.steps.sing", GLYPHS)}</S>,
            <S key={4} title={d.big.steps[4]}>{rich("big.steps.score", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("big.hood.app", { engine: src("engine.rs") })}</li>
            <li>{t("big.hood.local")}</li>
            <li>{rich("big.hood.frames", { dsp: src("dsp.rs") })}</li>
            <li>{rich("big.hood.learned", { store: src("store.rs") })}</li>
          </Hood>
        }
      />

      <Levels
        id="hearing" title={t("hearing.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.hearing.label} diagram={<Hearing text={d.hearing} />} steps={[
            <S key={0} title={d.hearing.steps[0]}>{rich("hearing.steps.apps", GLYPHS)}</S>,
            <S key={1} title={d.hearing.steps[1]}>{rich("hearing.steps.one", GLYPHS)}</S>,
            <S key={2} title={d.hearing.steps[2]}>{rich("hearing.steps.mic", GLYPHS)}</S>,
            <S key={3} title={d.hearing.steps[3]}>{rich("hearing.steps.memory", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("hearing.hood.capture", { captureSwift: file("src-tauri/native/capture.swift"), captureRs: src("capture.rs") })}</li>
            <li>{rich("hearing.hood.tapped", { source: src("source.rs") })}</li>
            <li>{t("hearing.hood.resampled")}</li>
            <li>{t("hearing.hood.permissions")}</li>
          </Hood>
        }
      />

      <Levels
        id="learning" title={t("learning.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.learning.label} diagram={<Learning text={d.learning} />} steps={[
            <S key={0} title={d.learning.steps[0]}>{rich("learning.steps.mixed", GLYPHS)}</S>,
            <S key={1} title={d.learning.steps[1]}>{rich("learning.steps.separation", GLYPHS)}</S>,
            <S key={2} title={d.learning.steps[2]}>{rich("learning.steps.pitch", GLYPHS)}</S>,
            <S key={3} title={d.learning.steps[3]}>{rich("learning.steps.notes", GLYPHS)}</S>,
            <S key={4} title={d.learning.steps[4]}>{rich("learning.steps.lyrics", GLYPHS)}</S>,
            <S key={5} title={d.learning.steps[5]}>{rich("learning.steps.bars", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("learning.hood.separation", { learn: src("learn.rs"), mdx: src("mdx.rs") })}</li>
            <li>{rich("learning.hood.firstPlay")}</li>
            <li>{rich("learning.hood.memory", { lowmem: src("lowmem.rs") })}</li>
            <li>{rich("learning.hood.pitch", { pitch: src("pitch.rs") })}</li>
            <li>{rich("learning.hood.notes", { game: src("game.rs"), syllables: src("syllables.rs") })}</li>
            <li>{rich("learning.hood.lyrics", { lyrics: src("lyrics.rs"), ctc: src("ctc.rs") })}</li>
            <li>{rich("learning.hood.measured")}</li>
            <li>{rich("learning.hood.kept")}</li>
          </Hood>
        }
      />

      <Levels
        id="placement" title={t("placement.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.placement.label} diagram={<Placement text={d.placement} />} steps={[
            <S key={0} title={d.placement.steps[0]}>{rich("placement.steps.fingerprint", GLYPHS)}</S>,
            <S key={1} title={d.placement.steps[1]}>{rich("placement.steps.slide", GLYPHS)}</S>,
            <S key={2} title={d.placement.steps[2]}>{rich("placement.steps.match", GLYPHS)}</S>,
            <S key={3} title={d.placement.steps[3]}>{rich("placement.steps.skip", GLYPHS)}</S>,
            <S key={4} title={d.placement.steps[4]}>{rich("placement.steps.listening", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("placement.hood.fingerprint", { dsp: src("dsp.rs") })}</li>
            <li>{rich("placement.hood.placement", { tracker: src("tracker.rs") })}</li>
            <li>{rich("placement.hood.measured")}</li>
            <li>{rich("placement.hood.listening", { listen: src("listen.rs"), recognize: src("recognize.rs") })}</li>
            <li>{t("placement.hood.shazamTest")}</li>
          </Hood>
        }
      />

      <Levels
        id="you" title={t("you.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.echo.label} diagram={<Echo text={d.echo} />} steps={[
            <S key={0} title={d.echo.steps[0]}>{rich("you.steps.speakers", GLYPHS)}</S>,
            <S key={1} title={d.echo.steps[1]}>{rich("you.steps.copy", GLYPHS)}</S>,
            <S key={2} title={d.echo.steps[2]}>{rich("you.steps.learns", GLYPHS)}</S>,
            <S key={3} title={d.echo.steps[3]}>{rich("you.steps.subtracts", GLYPHS)}</S>,
            <S key={4} title={d.echo.steps[4]}>{rich("you.steps.headphones", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("you.hood.livePitch", { pitch: src("pitch.rs") })}</li>
            <li>{rich("you.hood.canceller", { bleed: src("bleed.rs") })}</li>
            <li>{rich("you.hood.measured")}</li>
            <li>{rich("you.hood.limit")}</li>
          </Hood>
        }
      />

      <Levels
        id="scoring" title={t("scoring.title")} hoodTitle={hoodTitle}
        how={
          <>
            <Scrolly {...scrolly} label={d.scoring.label} diagram={<ScoreDiagram text={d.scoring} />} steps={[
              <S key={0} title={d.scoring.steps[0]}>{rich("scoring.steps.compare", GLYPHS)}</S>,
              <S key={1} title={d.scoring.steps[1]}>{rich("scoring.steps.credit", GLYPHS)}</S>,
              <S key={2} title={d.scoring.steps[2]}>{rich("scoring.steps.octaves", GLYPHS)}</S>,
              <S key={3} title={d.scoring.steps[3]}>{rich("scoring.steps.slack", GLYPHS)}</S>,
              <S key={4} title={d.scoring.steps[4]}>{rich("scoring.steps.hit", GLYPHS)}</S>,
              <S key={5} title={d.scoring.steps[5]}>{rich("scoring.steps.score", GLYPHS)}</S>,
            ]} />
            <div className="wide mt-8 grid gap-5 xl:grid-cols-2">
              <CreditPlayground locale={locale} text={d.credit} />
              <CoveragePlayground locale={locale} text={d.coverage} />
            </div>
            <p>
              {t("scoring.lenient")}
            </p>
            <p>{rich("scoring.explained", { link: pageLink(locale, "/docs/scoring/") })}</p>
          </>
        }
        hood={
          <Hood>
            <li>{rich("scoring.hood.credit", { dsp: src("dsp.rs") })}</li>
            <li>{rich("scoring.hood.slack")}</li>
            <li>{rich("scoring.hood.delay", { analysis: src("analysis.rs") })}</li>
            <li>{rich("scoring.hood.notesHit")}</li>
            <li>{rich("scoring.hood.score")}</li>
            <li>{rich("scoring.hood.experts")}</li>
          </Hood>
        }
      />

      <Levels
        id="privacy" title={t("privacy.title")} hoodTitle={hoodTitle}
        how={
          <Scrolly {...scrolly} label={d.privacy.label} diagram={<Privacy text={d.privacy} />} steps={[
            <S key={0} title={d.privacy.steps[0]}>{rich("privacy.steps.kept", GLYPHS)}</S>,
            <S key={1} title={d.privacy.steps[1]}>{rich("privacy.steps.never", GLYPHS)}</S>,
            <S key={2} title={d.privacy.steps[2]}>{rich("privacy.steps.lookups", GLYPHS)}</S>,
            <S key={3} title={d.privacy.steps[3]}>{rich("privacy.steps.shazam", GLYPHS)}</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>{rich("privacy.hood.stored", { dataDir: DATA_DIR })}</li>
            <li>{t("privacy.hood.network")}</li>
            <li>{rich("privacy.hood.more", { link: pageLink(locale, "/docs/privacy/") })}</li>
          </Hood>
        }
      />
    </>
  );
}
