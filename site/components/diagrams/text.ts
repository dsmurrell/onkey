// The diagrams' words in the page's language, from docs.diagrams in the catalog. The diagrams run in the browser
// (they animate), so they can't read the catalog themselves: a server component builds this and passes it in.
// Server only (lib/i18n reads the catalogs).
import { getT } from "@/lib/i18n";
import { PHRASE } from "./song";
import type { BigText } from "./BigPicture";
import type { HearingText } from "./Hearing";
import type { LearningText } from "./Learning";
import type { PlacementText } from "./Placement";
import type { EchoText } from "./Echo";
import type { ScoreText, CreditText, CoverageText } from "./Scoring";
import type { PrivacyText } from "./Privacy";

export type DiagramText = {
  big: BigText; hearing: HearingText; learning: LearningText; placement: PlacementText; echo: EchoText;
  scoring: ScoreText; credit: CreditText; coverage: CoverageText; privacy: PrivacyText;
  /** "Step {n} of {total}", as written (raw ICU), for the Scrolly's counter */
  stepOf: string;
};

export function diagramText(locale: string): DiagramText {
  const { t } = getT(locale, "docs.diagrams");
  // the made-up lyric the diagrams sing: one syllable per note of PHRASE, or English when a translation's count differs
  const phrase = t("phrase").split(" ");
  const words = phrase.length === PHRASE.length ? phrase : PHRASE.map((p) => p[3]);
  return {
    stepOf: t.raw("stepOf"),
    big: {
      label: t("big.label"), all: t("big.all"), onMac: t("big.onMac"),
      steps: [t("big.steps.play"), t("big.steps.hears"), t("big.steps.learns"), t("big.steps.sing"), t("big.steps.score")],
      musicApp: t("big.musicApp"), musicAppSub: t("big.musicAppSub"), hears: t("big.hears"), hearsSub: t("big.hearsSub"),
      learns: t("big.learns"), learnsSub: t("big.learnsSub"), sing: t("big.sing"), singSub: t("big.singSub"),
      score: t("big.score"), scoreSub: t("big.scoreSub"),
    },
    hearing: {
      label: t("hearing.label"),
      steps: [t("hearing.steps.apps"), t("hearing.steps.one"), t("hearing.steps.mic"), t("hearing.steps.memory")],
      musicApp: t("hearing.musicApp"), browser: t("hearing.browser"), messages: t("hearing.messages"), yourMic: t("hearing.yourMic"),
      oneClock: t("hearing.oneClock"), disk: t("hearing.disk"), noMusicSaved: t("hearing.noMusicSaved"), notHeard: t("hearing.notHeard"),
    },
    learning: {
      label: t("learning.label"),
      steps: [t("learning.steps.mixed"), t("learning.steps.separation"), t("learning.steps.pitch"), t("learning.steps.notes"), t("learning.steps.lyrics"), t("learning.steps.bars")],
      singer: t("learning.singer"), everythingElse: t("learning.everythingElse"), mix: t("learning.mix"), timing: t("learning.timing"), words,
    },
    placement: {
      label: t("placement.label"),
      steps: [t("placement.steps.fingerprint"), t("placement.steps.slide"), t("placement.steps.match"), t("placement.steps.skip"), t("placement.steps.listening")],
      fingerprint: t("placement.fingerprint"), match: t("placement.match"), playingNow: t("placement.playingNow"), skipped: t("placement.skipped"),
      shazam: t("placement.shazam"), shazamNote: t("placement.shazamNote"),
    },
    echo: {
      label: t("echo.label"),
      steps: [t("echo.steps.speakers"), t("echo.steps.copy"), t("echo.steps.learns"), t("echo.steps.subtracts"), t("echo.steps.headphones")],
      speakers: t("echo.speakers"), headphones: t("echo.headphones"), yourMic: t("echo.yourMic"), whatsLeft: t("echo.whatsLeft"),
      micHears: t("echo.micHears"), copy: t("echo.copy"), room: t("echo.room"), minusSong: t("echo.minusSong"),
    },
    scoring: {
      label: t("scoring.label"),
      steps: [t("scoring.steps.compare"), t("scoring.steps.credit"), t("scoring.steps.octaves"), t("scoring.steps.slack"), t("scoring.steps.hit"), t("scoring.steps.score")],
      flat60: t("scoring.flat60"), octaveLower: t("scoring.octaveLower"), slack80: t("scoring.slack80"), summary: t("scoring.summary"), words,
    },
    credit: {
      title: t("credit.title"), aria: t.raw("credit.aria"), axis: t("credit.axis"), credit: t.raw("credit.credit"),
      yourNote: t("credit.yourNote"), onTheNote: t("credit.onTheNote"), off: t.raw("credit.off"), octave: t("credit.octave"),
    },
    coverage: {
      title: t("coverage.title"), aria: t.raw("coverage.aria"), axis: t("coverage.axis"), score: t.raw("coverage.score"), youSang: t("coverage.youSang"),
    },
    privacy: {
      label: t("privacy.label"), all: t("privacy.all"), yourMac: t("privacy.yourMac"),
      steps: [t("privacy.steps.kept"), t("privacy.steps.never"), t("privacy.steps.lookups"), t("privacy.steps.shazam")],
      kept: [t("privacy.kept.notes"), t("privacy.kept.fingerprint"), t("privacy.kept.lyrics"), t("privacy.kept.takes")],
      audio: t("privacy.audio"), lrclib: t("privacy.lrclib"), itunes: t("privacy.itunes"), shazam: t("privacy.shazam"),
    },
  };
}
