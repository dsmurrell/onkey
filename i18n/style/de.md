# German (de) — OnKey style guide
Level: Full. Variant: standard German (Germany), fine for Austria and Switzerland. Use ß; for Swiss readers ß→ss is not needed.

## Register
- **du**, always. Spotify DE ("Finde den Soundtrack für deine Stimmung"), Apple Music DE ("Wähl ein Abo, das am besten zu dir passt") and macOS de ("Möchtest du…") all use du.
- Microsoft's guide still defaults to *Sie*, but OnKey is a consumer music app, so du is right.
- Capitalise *du*/*dein* only at the start of a sentence.
- Buttons, menu items and tabs use the infinitive, as macOS does ("Erlauben", "Beenden & erneut öffnen"): *Singen starten*, *Fortsetzen*, *Später einrichten*, *Nicht mehr anzeigen*.
- Instructions in sentences use the du imperative: "Spiel einen Song in Spotify ab", "dreh die Musik etwas leiser".

## Punctuation and typography
- German quotes „…“ (macOS de and Microsoft), apostrophe ’, ellipsis … with no space before. Drop most English exclamation marks.
- Nouns are capitalised. Everything else follows sentence case ("Mini-Modus", "Problem melden").
- Join compounds into one word or with a hyphen, never with a space: *Silbenbalken*, *Echounterdrückung*, *Mini-Modus*.
- An extended infinitive needs a comma ("…, um fair zu bewerten").
- Keep " · " with a space on each side. No full stop on buttons, tabs or short labels.

## Numbers, units, dates
- ICU formats numbers (decimal comma, thousands point): never hard-code separators in literal text. In literal text write "35 %" with a non-breaking space (DIN 5008, unverified), "35 Minuten", "5 s", the 24-hour clock ("21:30") and dates as dd.mm.yyyy.

## Plurals
- CLDR de: **one** (1 only) and **other**. **0 is other**: "0 Songs".

## Terms
| English | German | Note |
|---|---|---|
| take / Blind take | Take / Blind-Take | *der Take*, plural *die Takes* (studio word). "Jeder Song, den du singst, wird zu einem Take." |
| learn / learned / partly learned | lernen / gelernt / teilweise gelernt | "Gelernt · bereit zum Singen". "Learn it again" → *Neu lernen*. |
| lane | Notenbahn | |
| notes / syllable bars | Noten / Silbenbalken | |
| pitch line | Stimmlinie | |
| guide / Full guide | Notenhilfe / Volle Notenhilfe | "Fade the guide" → *Notenhilfe ausblenden*. |
| score / Scoring level | Punktzahl / Bewertungsstufe | Use *Punkte* in tight spots. |
| on key / On pitch | im Ton / ✓ Ton getroffen | |
| Sing · Library · History · Settings | Singen · Bibliothek · Verlauf · Einstellungen | |
| Songs to try / starter playlist | Songs zum Ausprobieren / Einstiegs-Playlist | *Song* and *Playlist*, as Spotify DE does. |
| Report a problem | Problem melden | |
| Challenge a friend | Freunde herausfordern | |
| Favourite (♥) | Favorit / Als Favoriten markieren | As in Apple Music de. |
| Mini mode / dancer | Mini-Modus / Tanzfigur | *Tanzfigur* is gender-neutral (not *Tänzer*). |
| Echo cancel | Echounterdrückung | |
| voice range / semitone / timing check | Stimmumfang / Halbton / Timing-Test | |
| Listening paused · Resume | Zuhören pausiert · Fortsetzen | |
| What's new | Neuigkeiten | |
| macOS labels | Systemeinstellungen · Datenschutz & Sicherheit · Mikrofon · Aufnahme von Bildschirm & Systemaudio | Exact macOS de strings. Keep the "&". |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Musik*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- *Sie*, nominal style (*Durchführung der Analyse*) and the passive voice.
- Gender star, colon or slash forms (*Sänger*innen*). Use neutral words instead (*alle, die singen*, *Freunde*); Microsoft DE now advises the same.
- Denglisch beyond *Song*, *Playlist* and *Take* (no *checken*, *downloaden*).

## Length
German runs about 30% longer than English, and compounds can't wrap. Keep buttons short (*Fortsetzen*, not *Zuhören fortsetzen*) and test the longest strings in Mini mode.

## Sources
- Microsoft German Style Guide, https://aka.ms/german-styleguide (PDF: quotes, comma rule, gender-neutral approach).
- https://www.spotify.com/de/premium/ and https://www.apple.com/de/apple-music/ (both du).
- macOS 26.6 system strings for de (SecurityPrivacyExtension, System Settings, Music and MusicUI `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
