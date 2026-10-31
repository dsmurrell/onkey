"use client";
import Link from "next/link";
import { Levels } from "@/components/explain/Levels";
import { Scrolly } from "@/components/explain/Scrolly";
import { Glyph } from "@/components/explain/Glyph";
import { codeUrl } from "@/lib/site";
import { BIG_STEPS, BigPicture } from "@/components/diagrams/BigPicture";
import { HEAR_STEPS, Hearing } from "@/components/diagrams/Hearing";
import { LEARN_STEPS, Learning } from "@/components/diagrams/Learning";
import { PLACE_STEPS, Placement } from "@/components/diagrams/Placement";
import { ECHO_STEPS, Echo } from "@/components/diagrams/Echo";
import { CoveragePlayground, CreditPlayground, SCORE_STEPS, ScoreDiagram } from "@/components/diagrams/Scoring";
import { PRIVACY_STEPS, Privacy } from "@/components/diagrams/Privacy";


/** A link to a source file, for "Under the hood". */
function Code({ path, children }: { path: string; children?: React.ReactNode }) {
  return <a href={codeUrl(path)}><code>{children ?? path.split("/").pop()}</code></a>;
}

/** "Under the hood": the exact facts, with where they live in the code. */
function Hood({ children }: { children: React.ReactNode }) {
  return <ul className="!mt-0 max-w-[65ch]">{children}</ul>;
}

const S = ({ title, children }: { title: string; children: React.ReactNode }) => (
  <><p className="!mt-0 text-sm font-medium uppercase tracking-wider text-faint">{title}</p><p>{children}</p></>
);

export function HowSections() {
  return (
    <>
      <Levels
        id="big-picture" title="1. The big picture"
        how={
          <Scrolly label="How OnKey works, end to end" diagram={(s) => <BigPicture step={s} />} steps={[
            <S key={0} title={BIG_STEPS[0]}>You play music the way you already do, in <strong>Spotify, Apple Music, or YouTube</strong>. OnKey doesn't play anything itself.</S>,
            <S key={1} title={BIG_STEPS[1]}>OnKey <strong>hears the music app</strong> <Glyph kind="wave" /> through macOS. The sound is analysed as it plays and never saved.</S>,
            <S key={2} title={BIG_STEPS[2]}>After a song's first play, OnKey <strong>learns it</strong>: it pulls out the singer's voice <Glyph kind="separate" />, works out the notes, and lines up the lyrics.</S>,
            <S key={3} title={BIG_STEPS[3]}>From the next play, you see the <strong>note bars</strong> <Glyph kind="lane" /> coming towards you, and your voice <Glyph kind="mic" /> drawn on top.</S>,
            <S key={4} title={BIG_STEPS[4]}>When the song ends you get a <strong>score</strong> <Glyph kind="score" /> and a report on each line, and whether you tend to sing sharp or flat.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>A Mac app: a Rust engine (audio, models, scoring) inside a Tauri window with a React interface. <Code path="src-tauri/src/engine.rs" /> runs everything.</li>
            <li>Everything happens on your Mac. Models run with ONNX Runtime, on Core ML (GPU and Neural Engine) where it helps.</li>
            <li>Audio is analysed at 16 kHz in 10 ms frames, the unit every number below uses (<Code path="src-tauri/src/dsp.rs" />, <code>FRAME_S</code>).</li>
            <li>A song counts as learned once 70% of it has been heard (<code>READY_COVERAGE</code> in <Code path="src-tauri/src/store.rs" />). A partly heard song keeps learning on later plays.</li>
          </Hood>
        }
      />

      <Levels
        id="hearing" title="2. Hearing the music"
        how={
          <Scrolly label="How OnKey hears the music" diagram={(s) => <Hearing step={s} />} steps={[
            <S key={0} title={HEAR_STEPS[0]}>Your Mac plays sound from lots of apps at once.</S>,
            <S key={1} title={HEAR_STEPS[1]}>OnKey asks macOS (with your permission) for the sound of <strong>one app</strong>, the music app, and ignores the rest.</S>,
            <S key={2} title={HEAR_STEPS[2]}>Your <strong>mic</strong> <Glyph kind="mic" /> is recorded alongside, on the <strong>same clock</strong>, so OnKey knows which moment of the song you sang against.</S>,
            <S key={3} title={HEAR_STEPS[3]}>The music is analysed <strong>as it plays</strong>, in memory (a song being learned stays in memory until it ends). It's never written to disk, uploaded, or shared.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>Capture uses Core Audio process taps (macOS 14.2+), with your mic, at your output device's own rate (usually 48 kHz), on one clock (<Code path="src-tauri/native/capture.swift" />, <Code path="src-tauri/src/capture.rs" />).</li>
            <li>Only the processes of the music app are tapped, chosen by its bundle id (<Code path="src-tauri/src/source.rs" />).</li>
            <li>Everything is resampled to 16 kHz mono for analysis. Release builds never write the music's audio anywhere. Only your own voice is kept with each take.</li>
            <li>Permissions: system audio recording and the microphone. Controlling Spotify and Music (what's playing and where, "Sing it again", skip, and pause) is a separate permission, asked in setup, and "Listen only" turns it off. Controlling the YouTube tab is off until you switch it on.</li>
          </Hood>
        }
      />

      <Levels
        id="learning" title="3. Learning a song"
        how={
          <Scrolly label="How OnKey learns a song" diagram={(s) => <Learning step={s} />} steps={[
            <S key={0} title={LEARN_STEPS[0]}>The song reaches OnKey as one sound: <strong>the singer and the band mixed together</strong>.</S>,
            <S key={1} title={LEARN_STEPS[1]}>Once the song has ended, a <strong>separation</strong> model <Glyph kind="separate" /> pulls the lead singer out of the mix, a chunk at a time. (Doing this while you sing would make the notes stutter.)</S>,
            <S key={2} title={LEARN_STEPS[2]}>A <strong>pitch tracker</strong> follows the singer's voice <strong>every 10 ms</strong>. That gives a wiggly line with the exact tune, slides and vibrato included.</S>,
            <S key={3} title={LEARN_STEPS[3]}>A note model turns that line into <strong>notes</strong>, with where each one starts and ends.</S>,
            <S key={4} title={LEARN_STEPS[4]}>The <strong>lyrics</strong> come from LRCLIB with rough timings. A speech model listens to the singer and moves each line to <strong>where its words are sung</strong>. On a first play it does this for the first minute and a half or so, so those lines are in time too.</S>,
            <S key={5} title={LEARN_STEPS[5]}>Each line is split into syllables, and each syllable gets its own <strong>bar</strong> <Glyph kind="lane" /> at its note. That's the lane you sing to.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li><strong>Separation:</strong> UVR MDX-Net Karaoke 2 (ONNX, on Core ML), run on 20 s chunks with 2 s of context either side. The music is held in memory while the song plays and separated after it ends, at a low priority. One song is learned at a time. Others wait their turn ("Waiting to learn"), and learning pauses while you sing a learned song with the window showing (<Code path="src-tauri/src/learn.rs" />, <Code path="src-tauri/src/mdx.rs" />).</li>
            <li><strong>First play:</strong> up to its first 90 s are separated as they play, 5 s at a time, until the lyric lines are lined up with the singer. The rest waits for the song to end. This is skipped on 8 GB Macs and while another song is being learned. Across my library, 60% of first-play lines were within 0.3 s of the singing this way, against 13% at LRCLIB's own times.</li>
            <li><strong>Memory:</strong> with 8 GB, one model is loaded at a time and unloaded after its step. Below 32 GB, the learning models (about 1.5 GB) are unloaded once a song is learned, unless Settings → Keep learning models ready is on (<Code path="src-tauri/src/lowmem.rs" />).</li>
            <li><strong>Pitch:</strong> CREPE "full" on the separated vocal, 16 kHz, 10 ms hop, 1024-sample windows, 50 to 1100 Hz. A frame counts as sung when its confidence is over 0.45 (<Code path="src-tauri/src/pitch.rs" />).</li>
            <li><strong>Notes:</strong> GAME (OpenVPI) transcribes notes from the vocal. Each syllable bar takes its note, and bars with no sung note under them are dropped (<Code path="src-tauri/src/game.rs" />, <Code path="src-tauri/src/syllables.rs" />).</li>
            <li><strong>Lyrics:</strong> synced lyrics from LRCLIB (looked up by artist, title, album, and length, then by name if that finds nothing). Lines are re-placed by aligning the whole song's words with wav2vec 2.0 (CTC), and split into syllables with the CMU pronouncing dictionary (<Code path="src-tauri/src/lyrics.rs" />, <Code path="src-tauri/src/ctc.rs" />).</li>
            <li><strong>Measured:</strong> on 18 hand-marked songs, 98% of sung notes are covered by a bar, 92% on the right note, and 73% starting within 0.1 s. 94% of lyric lines are within 0.3 s.</li>
            <li><strong>Kept with the song:</strong> the pitch line, the bars, a rhythm fingerprint, a Shazam signature and the lyrics' timing, plus the LRCLIB entry those times were made for (kept on the Mac, never uploaded), so the same words show on the same bars every play. The music and the separated vocal aren't kept.</li>
          </Hood>
        }
      />

      <Levels
        id="placement" title="4. Knowing where you are"
        how={
          <Scrolly label="How OnKey finds your place in the song" diagram={(s) => <Placement step={s} />} steps={[
            <S key={0} title={PLACE_STEPS[0]}>When a song is learned, OnKey also keeps a <strong>fingerprint</strong> <Glyph kind="fingerprint" />: how much the sound changes in each band of frequencies, every 10 ms.</S>,
            <S key={1} title={PLACE_STEPS[1]}>Music apps only roughly report where the song is, so OnKey slides <strong>a few seconds of what it's hearing</strong> along the fingerprint.</S>,
            <S key={2} title={PLACE_STEPS[2]}>Where they fit is <strong>exactly where you are</strong>. Until then the notes are dimmed ("Lining up with the song…"). They usually light up a few seconds into the song.</S>,
            <S key={3} title={PLACE_STEPS[3]}>Skip, seek, or restart, and it <strong>finds you again</strong> within a few seconds. OnKey keeps a few possible places in mind and only moves the notes when the music clearly agrees.</S>,
            <S key={4} title={PLACE_STEPS[4]}>This is <strong>listening mode</strong>. Songs you've sung before are recognised on your Mac, and Shazam names new ones, so OnKey can follow the music without controlling the app. Control of Spotify and Apple Music gives exact positions and is on by default ("Listen only" turns it off). YouTube control is opt-in.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li><strong>Fingerprint:</strong> how much the sound rises in each of 8 bands from about 60 Hz to 7 kHz, every 10 ms (<code>FP_BANDS</code> in <Code path="src-tauri/src/dsp.rs" />).</li>
            <li><strong>Placement:</strong> one tracker places every play (<Code path="src-tauri/src/tracker.rs" />, after Arzt and Widmer's "any-time" music tracker). Each check matches the last few seconds of music, at most 8 s, against the fingerprint, to within a 10 ms frame, and keeps the best few places as weighted candidates, so a repetitive intro that matches a beat off doesn't fool it. The music app's position and listening's answers only nudge the candidates near them. The shown place moves to another candidate only when that one clearly leads over about 4 s of music.</li>
            <li><strong>Measured:</strong> replaying 153 real plays with the music app's position wrong (0.73 s or 1.7 s off, or a skip landing 3.5 s away), the shown place was within 0.1 s of the music about 91% of the time. The rules before managed 75%, 0%, and 0%.</li>
            <li><strong>Listening mode</strong> (always on, <Code path="src-tauri/src/listen.rs" />, <Code path="src-tauri/src/recognize.rs" />): after 3 s of sound it asks, in order, OnKey's own catalogue of learned songs (ShazamKit, on the Mac, exact to about 10 ms), the fingerprints, and then Shazam's catalogue. It checks again every 3 s for learned songs and about every 12 s for songs only Shazam knows. Shazam gets a signature of at most 5 s of the music app's sound, never the mic. A new Shazam answer needs a second one that agrees. A pause is 0.15 s of digital silence.</li>
            <li>First test of Shazam's catalogue: it named 25 of 30 test songs from their first 5 s, in a median 0.24 s.</li>
          </Hood>
        }
      />

      <Levels
        id="you" title="5. Hearing you"
        how={
          <Scrolly label="How echo cancelling works" diagram={(s) => <Echo step={s} />} steps={[
            <S key={0} title={ECHO_STEPS[0]}>On laptop speakers, your mic hears <strong>you and the song</strong>, singer included. Left alone, the singer would score points for you.</S>,
            <S key={1} title={ECHO_STEPS[1]}>But OnKey has its own copy of the song <Glyph kind="wave" />, on the same clock as the mic.</S>,
            <S key={2} title={ECHO_STEPS[2]}>It <strong>learns how that sound reaches the mic</strong>, with the delay, the room's echo, and your speakers' tone. Video calls do the same.</S>,
            <S key={3} title={ECHO_STEPS[3]}>Then it <strong>subtracts</strong> <Glyph kind="echo" /> its prediction of the song, and what's left is <strong>your voice</strong>.</S>,
            <S key={4} title={ECHO_STEPS[4]}>With <strong>headphones</strong>, the mic doesn't hear the song, so there's nothing to take away. It's still the most reliable setup.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li><strong>Live pitch:</strong> CREPE "tiny", on your echo-cancelled mic, every 10 ms, run on Core ML's CPU kernels: the pitch dot is 2 to 4 ms behind your voice, and singing a learned song costs OnKey 15 to 19% of one CPU core on my Mac (<Code path="src-tauri/src/pitch.rs" />).</li>
            <li><strong>Echo canceller:</strong> a partitioned frequency-domain adaptive filter: 16 partitions of 256 samples at 16 kHz, so it models 256 ms of echo (<Code path="src-tauri/src/bleed.rs" />). Some outputs put the echo later than that, so every 2 s the echo's delay is measured (GCC-PHAT, up to 800 ms) and the song is fed to the filter that late. It only adapts while the song is clearly reaching the mic, and if its output comes out louder than the mic, the mic is passed through unchanged.</li>
            <li><strong>Measured:</strong> built-in MacBook speakers (echo about 24 ms late): 20 dB or more removed. External speakers with the echo about 290 ms late: 8 to 14 dB, up from 1 to 2 dB before the delay was measured.</li>
            <li><strong>Limit:</strong> 8 to 14 dB still leaves some of a loud song in the mic, and that can be mistaken for singing. Headphones are more reliable.</li>
          </Hood>
        }
      />

      <Levels
        id="scoring" title="6. Scoring"
        how={
          <>
            <Scrolly label="How OnKey scores your singing" diagram={(s) => <ScoreDiagram step={s} />} steps={[
              <S key={0} title={SCORE_STEPS[0]}>Every 10 ms that you and the singer both sing, your pitch is compared with the singer's line and the note bar, <strong>whichever you're closer to</strong>.</S>,
              <S key={1} title={SCORE_STEPS[1]}>Within <strong>50 cents</strong> (half a semitone) earns full credit. Credit then fades to nothing at 150 cents.</S>,
              <S key={2} title={SCORE_STEPS[2]}><strong>Octaves are forgiven</strong>, so you can sing in your own range.</S>,
              <S key={3} title={SCORE_STEPS[3]}>Each half-second may shift up to <strong>80 ms</strong> to line up with the singer, so being a little early or late doesn't cost you.</S>,
              <S key={4} title={SCORE_STEPS[4]}>Each note is <strong>hit</strong> if you hold it within a semitone for half its length. Its first 80 ms don't count: singers scoop up into notes.</S>,
              <S key={5} title={SCORE_STEPS[5]}>Your score <Glyph kind="score" /> is your <strong>accuracy</strong>, scaled by <strong>how much of the singer's part you sang</strong>, so one good line won't get a good score.</S>,
            ]} />
            <div className="wide mt-8 grid gap-5 xl:grid-cols-2">
              <CreditPlayground />
              <CoveragePlayground />
            </div>
            <p>
              By default the scoring is lenient on purpose, set for beginners like me. Good singers can pick a
              stricter level, Steady or Pro, in Settings (vibrato isn&rsquo;t marked down at either).
            </p>
            <p>The numbers in your report are explained in <Link href="/docs/scoring/">Scoring explained</Link>.</p>
          </>
        }
        hood={
          <Hood>
            <li><strong>Credit:</strong> <code>clamp((150 − |error|) / 100, 0, 1)</code>, with the error in cents folded into ±600 so octaves don't count (<code>credit</code>, <code>fold</code> in <Code path="src-tauri/src/dsp.rs" />).</li>
            <li><strong>Timing slack:</strong> each 0.5 s window gets the one shift within ±80 ms (<code>TOL</code> = 8 frames) that lines you up best (<code>tolerant_error</code>).</li>
            <li><strong>Your delay:</strong> after the song, a search from 0.3 s early to 1.5 s late finds the lag that fits best (<Code path="src-tauri/src/analysis.rs" />).</li>
            <li><strong>Notes hit:</strong> held within 100 cents for at least half the note, ignoring its first 80 ms. Notes under 0.15 s aren't counted.</li>
            <li><strong>Score:</strong> <code>100 × accuracy × (0.3 × min(1, sung / 0.1) + 0.7 × min(1, sung / 0.4))</code>: full credit from 40% of the singer's part sung.</li>
            <li><strong>Checked against experts:</strong> on 20 rated recordings, notes hit agrees with expert rankings at 0.77 and the score at 0.61 (experts agree with each other at 0.88).</li>
          </Hood>
        }
      />

      <Levels
        id="privacy" title="7. Privacy"
        how={
          <Scrolly label="What OnKey keeps and sends" diagram={(s) => <Privacy step={s} />} steps={[
            <S key={0} title={PRIVACY_STEPS[0]}>On your Mac, OnKey keeps what it <strong>learned</strong> about each song, and for each take <strong>a recording of your mic</strong> <Glyph kind="lock" /> (about 0.5 MB a minute), your scores and the lyrics you sang, so you can play it back. Delete a take and it goes to the Trash.</S>,
            <S key={1} title={PRIVACY_STEPS[1]}>It <strong>never saves or sends the music's audio</strong>, or the separated vocal. (If the music was on speakers, your mic recording will have some of it.)</S>,
            <S key={2} title={PRIVACY_STEPS[2]}>To find lyrics it sends the song's <strong>artist, title, album and length</strong> to LRCLIB, a free, community-run lyrics database (unless Show lyrics is off). For covers, it asks Apple's iTunes Search by artist and title.</S>,
            <S key={3} title={PRIVACY_STEPS[3]}>To name a song it hasn't learned, <strong>listening mode</strong> sends Shazam a <strong>signature</strong> of up to 5 seconds of the music app's sound (not the audio, and never your mic). Songs you've sung before are recognised on your Mac, so nothing goes to Shazam for them. Shazam is only asked when Spotify or Apple Music isn't saying what's playing, and you can turn it off in Settings.</S>,
          ]} />
        }
        hood={
          <Hood>
            <li>Stored in <code>~/Library/Application Support/audio.onkey.app/</code>: <code>songs/</code> (pitch line, bars, fingerprint, lyric timings and a Shazam signature, saved for every learned song in either mode), <code>takes/</code> (<code>mic.m4a</code>: your microphone, AAC mono at about 64 kbps, about 0.5 MB a minute, all kept up to 2 GB in total, plus scores and the report with the lyric lines) and <code>records/</code> (logs and a summary of each play, numbers only, kept 30 days, at most 200 MB). Deleting a song or take moves it to the Trash.</li>
            <li>Network: Shazam (listening mode, can be turned off) for songs not yet learned, LRCLIB lyric lookups (unless Show lyrics is off), iTunes Search for covers, iTunes Lookup for a recognised song's length, the cover images (from Apple, or YouTube's thumbnail), and a daily check of GitHub Releases for updates. No account, no analytics, and no server of mine.</li>
            <li>More in <Link href="/docs/privacy/">Privacy &amp; legal</Link>.</li>
          </Hood>
        }
      />
    </>
  );
}
