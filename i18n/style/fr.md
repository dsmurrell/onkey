# French (fr) — OnKey style guide
Level: Full. Variant: French (France), also readable in Belgium, Switzerland and Canada.

## Register
- **vous**, always. Decision: Spotify FR's own site ("Écoutez à votre façon"), Apple Music FR ("Vous conservez votre bibliothèque"), macOS fr ("Voulez-vous vraiment… ?") and Microsoft all use vous.
- Duolingo FR is said to use *tu* (unverified, its page didn't load). But OnKey sits right next to macOS dialogs and speaks to adults, so *vous* matches the system and won't jar.
- Never mix in *tu* or the impersonal *on*.
- Buttons, menu items and tabs use the infinitive, as macOS does ("Autoriser", "Quitter et rouvrir"): *Commencer à chanter*, *Reprendre*, *Configurer plus tard*, *Ne plus afficher*.
- Instructions in sentences use the vous imperative: "Lancez une chanson dans Spotify", "baissez un peu le volume".

## Punctuation and typography
- Put a non-breaking space (U+00A0, as macOS fr does) before **? ! : ;**, before **%**, inside guillemets (« texte ») and as the thousands separator.
- Guillemets « » for quotes, never “ ” (Microsoft and macOS fr). Apostrophe ’ (*l’écho*, *n’entend*). Ellipsis … with no space before.
- Sentence case for every label ("Mode mini", "Signaler un problème"). Accented capitals are required: *À*, *É*, *Ç*.
- Keep " · " with a space on each side. No full stop on buttons, tabs or short labels.
- Drop most English exclamation marks. French ones would also need the non-breaking space.

## Numbers, units, dates
- ICU formats numbers (decimal comma, non-breaking-space thousands). In literal text write "35 %" and "5 000" with non-breaking spaces, "35 minutes", the 24-hour clock ("21:30" in the UI), and dates as dd/mm/yyyy.

## Plurals
- CLDR fr: **one** covers **0 and 1** ("0 chanson", "1 chanson"), **many** covers exact millions (for compact numbers, "1 million de chansons"), and **other** covers everything else.
- Supply one and other, and check that one reads well with 0. If not, write a separate =0 case ("Aucune chanson").

## Terms
| English | French | Note |
|---|---|---|
| take / Blind take | prise / prise à l’aveugle | The studio word, feminine: "Chaque chanson que vous chantez devient une prise." |
| learn / learned / partly learned | apprendre / apprise / apprise en partie | Agrees with *chanson*: "Apprise · prête à chanter". "Learn it again" → *La réapprendre*. |
| lane | couloir | Not *piste*, which means a track. |
| notes / syllable bars | notes / barres de syllabes | |
| pitch line | ligne de voix | |
| guide / Full guide | guide / Guide complet | "Fade the guide" → *Estomper le guide*. |
| score / Scoring level | score / Niveau de notation | Not *note*, which would clash with musical notes. |
| on key / On pitch | juste / ✓ Juste | As in *chanter juste*. |
| Sing · Library · History · Settings | Chanter · Bibliothèque · Historique · Réglages | *Réglages*, as in macOS fr. |
| Songs to try / starter playlist | Chansons à essayer / playlist de départ | Use *chanson* for "song" throughout. |
| Report a problem | Signaler un problème | |
| Challenge a friend | Lancer un défi | Gender-neutral (not *Défier un ami*). |
| Favourite (♥) | favori / Ajouter aux favoris | As in Apple Music fr. |
| Mini mode / dancer | Mode mini / danseur | |
| Echo cancel | Suppression de l’écho | |
| voice range / semitone / timing check | tessiture / demi-ton / test de synchronisation | |
| Listening paused · Resume | Écoute en pause · Reprendre | |
| What's new | Nouveautés | |
| macOS labels | Réglages Système · Confidentialité et sécurité · Micro · Enregistrement de l’écran et des sons du système | Exact macOS fr strings. |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Musique*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- Anglicisms where French has a plain word (*checker*, *uploader*, *setup*). *Playlist* and *score* are fine.
- Stiff forms: *Veuillez*, *Il est nécessaire de*, the passive voice.
- Inclusive writing with *·e* or *(e)*: rephrase instead (*les personnes*, *quiconque chante*).

## Length
French runs 15–30% longer than English. Keep buttons to 2–3 words. Remember that the non-breaking spaces count too.

## Sources
- Microsoft French (France) Style Guide, https://aka.ms/french-france-styleguide (PDF: vous, guillemets, non-breaking spaces).
- https://www.spotify.com/fr/premium/ and https://www.apple.com/fr/apple-music/ (both vous). https://fr.duolingo.com/ was fetched but returned no text.
- macOS 26.6 system strings for fr (SecurityPrivacyExtension, System Settings, Music and MusicUI `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
