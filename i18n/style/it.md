# Italian (it) — OnKey style guide
Level: Full. Variant: standard Italian (Italy).

## Register
- **tu**, always: Spotify IT ("Porta Spotify sempre con te"), macOS it ("non hai specificato…", "Confermi di voler…") and the Microsoft Italian voice all use it. Never *Lei* or *voi*.
- Buttons and menu items use the tu imperative, as macOS does ("Consenti", "Esci e riapri"): *Inizia a cantare*, *Riprendi*, *Configura più tardi*, *Non mostrare più*.
- Tabs are short nouns or imperatives (see Terms). Never mix infinitive and imperative buttons on one screen.

## Punctuation and typography
- Quotes “ ” (as macOS it); apostrophe ’ for elisions (*l’eco*, *un’altra*); ellipsis … with no space before. Punctuation goes outside the quotes (Microsoft). Drop most English exclamation marks.
- Sentence case for every label ("Modalità mini", "Segnala un problema").
- Accented capitals are real letters: *È*, never *E’*.
- Keep " · " with a space on each side. No full stop on buttons, tabs or short labels.

## Numbers, units, dates
- ICU formats numbers (decimal comma, thousands point): never hard-code separators in literal text. "35%" with no space; "35 minuti"; 24-hour clock ("21:30"); dates dd/mm/yyyy.

## Plurals
- CLDR it: **one** (1 only), **many** (exact millions, for compact numbers), **other**. **0 is other**: "0 brani".
- Supply one and other. Past participles agree with the noun: use *brano* (masculine) throughout so the agreement stays predictable.

## Terms
| English | Italian | Note |
|---|---|---|
| take / Blind take | esibizione / esibizione alla cieca | "Ogni brano che canti diventa un’esibizione." Clearer than the anglicism *take*. |
| learn / learned / partly learned | imparare / imparato / imparato in parte | "Imparato · pronto da cantare". "Learn it again" → *Imparalo di nuovo*. |
| lane | corsia | |
| notes / syllable bars | note / barre delle sillabe | |
| pitch line | linea della voce | |
| guide / Full guide | guida / Guida completa | "Fade the guide" → *Sfuma la guida*. |
| score / Scoring level | punteggio / Livello di punteggio | |
| on key / On pitch | intonato / ✓ Intonato | |
| Sing · Library · History · Settings | Canta · Libreria · Cronologia · Impostazioni | *Libreria* as in Apple Music it; *Impostazioni* as in macOS. |
| Songs to try / starter playlist | Brani da provare / playlist iniziale | Use *brano* for "song" (Spotify and Apple use it too). |
| Report a problem | Segnala un problema | |
| Challenge a friend | Lancia una sfida | Gender-neutral (not *Sfida un amico*). |
| Favourite (♥) | preferito / Aggiungi ai preferiti | As in Apple Music it. |
| Mini mode / dancer | Modalità mini / ballerino | |
| Echo cancel | Cancellazione dell’eco | |
| voice range / semitone / timing check | estensione vocale / semitono / test di sincronia | |
| Listening paused · Resume | Ascolto in pausa · Riprendi | |
| What's new | Novità | |
| macOS labels | Impostazioni di Sistema · Privacy e sicurezza · Microfono · Registrazione schermo e audio di sistema | Exact macOS it strings. |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Musica*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- Bureaucratic verbs (*effettuare*, *procedere a*, *si prega di*) and the impersonal *si* where *tu* reads naturally.
- Needless anglicisms (*settare*, *performance*, *skippare*), English title case, and calques such as "realizzare" for "realise".
- Generic masculine where a neutral form exists: *chi canta*, *le persone*. Never "amico/a" or "ə".

## Length
Italian runs 15–30% longer than English. Keep buttons to 2–3 words and use short verbs (*Riprendi*, not *Riprendi l’ascolto*).

## Sources
- Microsoft Italian Style Guide, https://aka.ms/italian-styleguide (PDF: voice, quotes, ellipsis).
- https://www.spotify.com/it/premium/ (tu).
- macOS 26.6 system strings for it (SecurityPrivacyExtension, System Settings, Music and MusicUI `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
