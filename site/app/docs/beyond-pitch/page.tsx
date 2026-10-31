import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "Beyond pitch" };

export default function BeyondPitch() {
  return (
    <>
      <h1>Beyond pitch</h1>
      <p className="lede">
        OnKey shows whether you're on the note. Singing well takes more than that, and the rest is best learned from
        a teacher.
      </p>

      <h2>What OnKey can and can't hear</h2>
      <p>
        OnKey tracks your pitch against the singer's, line by line. It's a quick way to hear, and see, where you drift.
        It can't tell how you made the sound: whether you're breathing well, straining, or singing with feeling.
        A high score means you hit the notes, not that you sang the song well.
      </p>

      <h2>What else to learn</h2>
      <ul>
        <li><strong>Breath support:</strong> steady air behind the sound, so long lines don't run out or wobble.</li>
        <li><strong>Less tension:</strong> a free throat, jaw and tongue. Reaching for high notes with strain is the usual way voices get tired or hurt.</li>
        <li><strong>Registers:</strong> chest voice, head voice and the mix between them, so you can move through your range without a break.</li>
        <li><strong>Vowels:</strong> shaping them so notes ring and high ones get easier.</li>
        <li><strong>Phrasing and rhythm:</strong> where to breathe, which words to lean on, and landing with the beat.</li>
      </ul>

      <h2>Free teachers to start with</h2>
      <ul>
        <li>
          <a href="https://www.youtube.com/@dancallawaystudio">Dan Callaway</a> teaches musical theatre voice at
          Boston Conservatory at Berklee. Clear lessons on technique, from breath to belting.
        </li>
        <li>
          <a href="https://www.youtube.com/@JeffRolka">Jeff Rolka</a>: warm-ups, singing in tune, high notes, runs
          and riffs.
        </li>
      </ul>
      <p>
        Both were suggested by singers in the discussion when OnKey launched. Other good teachers are out there,
        and a suggestion is welcome on <a href="https://github.com/dsmurrell/onkey/issues">GitHub</a>.
      </p>

      <h2>Lessons with a teacher</h2>
      <p>
        If you can, have a few lessons. A teacher hears in seconds what you'd take months to notice yourself, and
        sets you exercises for your voice. OnKey can then be how you practise in between: singing along to the songs
        you like, and seeing your pitch get steadier.
      </p>

      <p className="text-sm">
        For looking after your voice while you practise, see <Link href="/docs/your-voice/">Singing comfortably</Link>.
      </p>
    </>
  );
}
