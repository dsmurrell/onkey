# Dutch (nl) — OnKey style guide
Level: Early. Variant: standard Dutch (Netherlands), fine for Flanders.

## Register
- **je/jouw**, always. Spotify NL ("Je kunt opzeggen wanneer je wilt") and macOS nl ("Weet je zeker dat…") use je. Microsoft notes the shift from *u* to *je* in consumer products.
- Never *u*.
- Use *je* rather than *jij*, and *jouw* only for emphasis ("je score", "jouw stem" for contrast).
- Buttons and menu items use the imperative, as macOS does ("Sta toe", "Stop en open opnieuw"): *Begin met zingen*, *Ga verder*, *Stel later in*, *Toon niet meer*.
- Tabs are infinitives or nouns (see Terms). Never mix imperative and infinitive buttons on one screen.

## Punctuation and typography
- Quotes ‘ ’ for names and titles (Microsoft uses single quotes) and “ ” for real quotations. Apostrophe ’ (*foto’s*, *’s avonds*). Ellipsis … with no space before.
- Sentence case for every label ("Minimodus", "Meld een probleem").
- Join compounds into one word (*lettergreepbalkjes*, *echo-onderdrukking*). Use a hyphen only where two vowels clash or for readability. Keep the trema (*geïnstalleerd*).
- Keep " · " with a space on each side. No full stop on buttons, tabs or short labels. Drop most English exclamation marks.

## Numbers, units, dates
- ICU formats numbers (decimal comma, thousands point): never hard-code separators in literal text. "35%" with no space; "35 minuten"; 24-hour clock ("21:30"); dates d-m-yyyy.

## Plurals
- CLDR nl: **one** (1 only) and **other**. **0 is other**: "0 nummers".

## Terms
| English | Dutch | Note |
|---|---|---|
| take / Blind take | take / blinde take | *de take*, plural *takes* (studio word). "Elk nummer dat je zingt, wordt een take." |
| learn / learned / partly learned | leren / geleerd / deels geleerd | "Geleerd · klaar om te zingen". "Learn it again" → *Opnieuw leren*. |
| lane | notenbaan | |
| notes / syllable bars | noten / lettergreepbalkjes | |
| pitch line | stemlijn | |
| guide / Full guide | notenhulp / Volledige notenhulp | "Fade the guide" → *Notenhulp afbouwen*. |
| score / Scoring level | score / Scoreniveau | |
| on key / On pitch | zuiver / ✓ Zuiver | As in *zuiver zingen*. |
| Sing · Library · History · Settings | Zingen · Bibliotheek · Geschiedenis · Instellingen | |
| Songs to try / starter playlist | Nummers om te proberen / startplaylist | Use *nummer* for "song", as Spotify NL does. |
| Report a problem | Meld een probleem | |
| Challenge a friend | Daag iemand uit | Gender-neutral. |
| Favourite (♥) | favoriet / Maak favoriet | As in Apple Music nl. |
| Mini mode / dancer | Minimodus / danser | |
| Echo cancel | Echo-onderdrukking | |
| voice range / semitone / timing check | stembereik / halve toon / timingtest | |
| Listening paused · Resume | Luisteren gepauzeerd · Ga verder | |
| What's new | Wat is er nieuw | |
| macOS labels | Systeeminstellingen · Privacy en beveiliging · Microfoon · Scherm- en systeemaudio-opname | Exact macOS nl strings. |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Muziek*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- *u*, officialese (*dient te*, *middels*, *teneinde*) and the passive voice.
- Writing compounds as two words in English style (*echo onderdrukking* is wrong).
- Needless anglicisms (*checken*, *settings*). *Playlist*, *score* and *take* are fine.
- Gendered forms: *danser* and *iemand* are fine as neutral words.

## Length
Dutch runs 20–30% longer than English (Microsoft). Keep buttons to 2–3 words, but don't drop articles and prepositions just to save space.

## Sources
- Microsoft Dutch Style Guide, https://aka.ms/dutch-styleguide (PDF: u → je trend, single quotes, length).
- https://www.spotify.com/nl/premium/ (je/jouw).
- macOS 26.6 system strings for nl (SecurityPrivacyExtension, System Settings, Music and MusicUI `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
