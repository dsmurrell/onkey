import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "Singing comfortably" };

export default function YourVoice() {
  return (
    <>
      <h1>Singing comfortably</h1>
      <p className="lede">OnKey makes it easy to sing song after song, so your voice needs a little looking after.</p>

      <h2>Sing at a comfortable volume</h2>
      <p>
        OnKey only scores your pitch, so there's no need to sing over the music. Singing softly scores the same.
        Headphones help, as you don't have to compete with the speakers to hear yourself.
      </p>

      <h2>Drop an octave when a note is out of reach</h2>
      <p>
        If a song sits too high or low for you, sing it an octave lower or higher. OnKey counts the right note in any
        octave, so you lose nothing. Straining at the edge of your range is the quickest way to tire your voice.
      </p>

      <h2>Warm up and take breaks</h2>
      <ul>
        <li>Start with a few easy songs, or hum gently up and down your range for a minute or two.</li>
        <li>Keep water nearby.</li>
        <li>
          After about 35 minutes of singing, OnKey reminds you to rest (you can turn this off in <strong>Settings →
          Your voice</strong>).
        </li>
        <li>Build up gradually. A few hours on your first day is a lot more than most voices are used to.</li>
      </ul>

      <h2>Stop if it hurts</h2>
      <p>
        Pain, a scratchy throat, or a voice that's still hoarse the next day mean it's time to rest. If it lasts more
        than a couple of weeks, see a doctor or a voice specialist.
      </p>

      <p>
        For technique (breath, registers, singing without strain), see <Link href="/docs/beyond-pitch/">Beyond
        pitch</Link>.
      </p>

      <p className="text-sm">This page is general advice about singing, not medical advice.</p>
    </>
  );
}
