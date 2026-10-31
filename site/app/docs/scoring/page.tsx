import type { Metadata } from "next";
import { CoveragePlayground, CreditPlayground } from "@/components/diagrams/Scoring";

export const metadata: Metadata = { title: "Scoring explained" };

export default function Scoring() {
  return (
    <>
      <h1>Scoring explained</h1>
      <p className="lede">What each number means, how it's worked out, and where it falls short.</p>

      <h2>The rules</h2>
      <ul>
        <li>
          <strong>Every 10 ms counts.</strong> Whenever you and the singer are both singing, your pitch is compared
          with the singer's pitch line and the note bar, whichever you're closer to.
        </li>
        <li>
          <strong>Pitch credit.</strong> Within 50 cents (half a semitone) gets full credit. It then falls in a
          straight line to nothing at 150 cents.
        </li>
        <li>
          <strong>Octaves are forgiven.</strong> An octave above or below the singer counts as on the note, so you can
          sing in your own range.
        </li>
        <li>
          <strong>Timing slack.</strong> Each half-second can shift up to 80 ms to line up with the singer, so being
          a touch early or late doesn't cost you anything.
        </li>
        <li>
          <strong>Your delay is found for you.</strong> After the song, OnKey tries delays from 0.3 s early to 1.5 s
          late and uses the one that lines you up best (you hear the song a little late).
        </li>
      </ul>
      <p>
        These are the rules at <strong>Relaxed</strong>, the default level. They're forgiving on purpose, for
        beginners (I'm one) and children, so if you sing well you'll score high.
      </p>

      <h2>Levels</h2>
      <p>
        If Relaxed is too kind, pick a stricter level in Settings → Scoring. It applies from your next take, and each
        take keeps the level it was scored at: takes not at Relaxed show a small tag, and a song's best score only
        compares takes at the same level.
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>Level</th><th className="num">Full credit within</th><th className="num">No credit from</th><th className="num">Timing slack</th></tr></thead>
          <tbody>
            <tr><td><strong>Relaxed</strong> (default)</td><td className="num">50 cents</td><td className="num">150 cents</td><td className="num">80 ms</td></tr>
            <tr><td><strong>Steady</strong></td><td className="num">35 cents</td><td className="num">100 cents</td><td className="num">60 ms</td></tr>
            <tr><td><strong>Pro</strong></td><td className="num">25 cents</td><td className="num">70 cents</td><td className="num">50 ms</td></tr>
          </tbody>
        </table>
      </div>
      <p>
        <strong>Vibrato isn't penalised.</strong> At Steady and Pro your pitch is judged by the middle of its wobble,
        averaged over 0.17 s (about one cycle of a typical vibrato, which swings 5 to 7 times a second). A steady
        vibrato around the right note scores close to full; a note held sharp or flat is still sharp or flat. At
        Relaxed your exact pitch is used, as it always was, and a wide vibrato (±80 cents) loses about a tenth of
        its credit. Octaves are forgiven at every level.
      </p>

      <CreditPlayground />

      <h2>The numbers in your report</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>Number</th><th>What it means</th></tr></thead>
          <tbody>
            <tr><td><strong>Score</strong> (0–100)</td><td>Your average pitch credit, scaled by how much of the singer's part you sang (see below).</td></tr>
            <tr><td><strong>Pitch accuracy</strong></td><td>Your average pitch credit while you were singing.</td></tr>
            <tr><td><strong>Notes hit</strong></td><td>Notes you held within a semitone for at least half their length, with the same 80 ms slack. Each note's first 80 ms is ignored, as singers scoop up into notes, and notes under 0.15 s aren't counted.</td></tr>
            <tr><td><strong>Sang along</strong></td><td>The share of the singer's part where you were singing too.</td></tr>
            <tr><td><strong>Tendency</strong></td><td>Whether you tend to sit sharp or flat, in cents (the median of your errors).</td></tr>
            <tr><td><strong>Latency</strong></td><td>The delay OnKey found and allowed for.</td></tr>
          </tbody>
        </table>
      </div>

      <h3>How "sang along" affects the score</h3>
      <p>
        Score = 100 × pitch accuracy × a factor for how much you sang. Singing a tenth of the singer's part earns
        30% of that factor, and the rest builds up until you've sung 40% of it (some of the vocal is backing vocals
        or ad-libs nobody sings). So one good line won't get a good score, and someone who just listens scores close
        to nothing. The live score uses the same formula.
      </p>
      <CoveragePlayground />

      <h2>How I check the score is right</h2>
      <p>
        Every change to OnKey is measured before it's kept. Note bars are checked against hand-marked notes, lyric
        timing against hand-timed lyrics, and the score against simulated singers (perfect, wobbly, sharp, and
        silent with the song leaking into the mic) and against the original singers scored as if they were you.
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>Check</th><th className="num">Result</th></tr></thead>
          <tbody>
            <tr><td>Hand-marked notes that a note bar covers</td><td className="num">98%</td></tr>
            <tr><td>…whose bar is on the right note</td><td className="num">92%</td></tr>
            <tr><td>…whose bar starts within 0.1 s</td><td className="num">73%</td></tr>
            <tr><td>Bars with no sung note under them</td><td className="num">3%</td></tr>
            <tr><td>Lyric lines placed within 0.3 s</td><td className="num">91–94%</td></tr>
          </tbody>
        </table>
      </div>

      <h3>Does the score agree with experts?</h3>
      <p>
        I scored 20 recordings of people singing, from poor to professional, that music experts had already rated
        (the PESnQ research set), to see if OnKey ranks them in the same order. On a scale where 1 is perfect
        agreement:
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>Measure</th><th className="num">Agreement with experts</th></tr></thead>
          <tbody>
            <tr><td>Experts with each other (the realistic ceiling)</td><td className="num">0.88</td></tr>
            <tr><td>OnKey's notes hit</td><td className="num">0.77</td></tr>
            <tr><td>OnKey's score</td><td className="num">0.61</td></tr>
          </tbody>
        </table>
      </div>
      <p>
        Notes hit already follows the experts well, and the score could lean on it more. It's only 20 recordings,
        sung without backing tracks, so take it as a good sign more than proof.
      </p>

      <h2>Limits</h2>
      <ul>
        <li><strong>Rapped or spoken songs:</strong> pitch scoring barely applies.</li>
        <li><strong>Harmonies and choirs:</strong> it isn't always clear which voice is the tune, so some bars may follow a harmony.</li>
        <li><strong>Singing without headphones:</strong> echo cancelling removes 20 dB or more of the song on a MacBook's speakers, and 8 to 14 dB on external speakers. What's left of a loud song can be mistaken for singing, so headphones are more reliable.</li>
        <li><strong>The first play</strong> has no notes. They appear from the second play.</li>
      </ul>
    </>
  );
}
