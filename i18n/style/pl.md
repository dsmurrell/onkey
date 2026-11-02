# Polish (pl) — OnKey style guide
Level: Early. Variant: standard Polish (Poland).

## Register
Address the user with the informal **ty**, never Pan/Pani or "Użytkownik". Spotify PL and Apple Music PL both use ty. When a pronoun referring to the user does appear, capitalise it: **Ty, Twój, Twoje, Cię, Ciebie, Tobie**. Apple Music PL, Spotify PL ("czeka na Ciebie") and the Microsoft Polish guide all do this. Drop the pronoun where you can, as Polish verbs already show the person.
- Buttons and actions: 2nd-person singular imperative (`Zacznij śpiewać`, `Zgłoś problem`, `Nie pokazuj ponownie`, `Skonfiguruj później`, `Uruchom ponownie, aby zaktualizować`).
- **No gendered past tense addressed to the user**, and no "Użytkownik musi…" to dodge it (the Microsoft guide rejects that too). Write `Nie pamiętasz hasła?`, not `Zapomniałeś hasła?`. Use these constructions instead:
  - present or future tense: `Śpiewasz za cicho`, `Następnym razem zobaczysz każdą nutę`
  - a noun: `Twój wynik: 82%`, `Twoje podejścia`
  - impersonal -no/-to: `Zapisano`, `Nauczono się piosenki`
  - OnKey or the song as the subject: `OnKey nauczył się piosenki`

## Punctuation and typography
- Quotes „ ” (low then high), with « » inside. A full stop goes outside the closing quote.
- Ellipsis is the single character …, attached to the word before it.
- Use a spaced en dash – as the sentence dash.
- Sentence case for all labels. No full stop on buttons, tabs or short labels. Keep " · " as the separator.

## Numbers, units, dates
Decimal comma (5,5), thousands separated by a non-breaking space (10 000). Write the percent sign without a space: 85% (unverified for UI; both forms are seen). Use the 24-hour clock (14:30) and `35 minut`.

## Plurals
CLDR categories: **one, few, many, other** (other is used for fractions). Fill all four:
`{n, plural, one {# piosenka} few {# piosenki} many {# piosenek} other {# piosenki}}` (1 piosenka, 2–4/22 piosenki, 5–21/35 piosenek, 1,5 piosenki).
Watch these: 12–14 are many (`12 piosenek`), and 22–24 are few. 0 is many (`0 piosenek`), but `Brak piosenek` often reads better.

## Terms
| English | Polish | Note |
|---|---|---|
| take / Blind take | podejście / Podejście na ślepo | `Każda zaśpiewana piosenka to kolejne podejście` |
| learn / learned / Learn it again / partly learned | nauczyć się / nauczona / Naucz się jeszcze raz / nauczona częściowo | agrees with `piosenka` (fem.): `Nauczona · gotowa do śpiewania` |
| lane / notes / syllable bars | tor / nuty / paski sylab | |
| pitch line | linia głosu | |
| guide / Fade the guide / Full guide | podpowiedzi / Wygaszaj podpowiedzi / Pełne podpowiedzi | |
| score / Scoring level | wynik / Poziom oceniania | |
| on key / On pitch | czysto / czysto w punkt | `śpiewać czysto` is the natural phrase |
| Sing · Library · History · Settings | Śpiewaj · Biblioteka · Historia · Ustawienia | |
| Songs to try / starter playlist | Piosenki na start / playlista startowa | |
| Report a problem / Challenge a friend | Zgłoś problem / Rzuć wyzwanie znajomemu | |
| Favourite (♥) / What's new | Ulubione / Co nowego | |
| Mini mode / dancer | Tryb mini / tancerz | the dancer is a figure, so masculine is fine |
| Echo cancel / voice range / semitone | Usuwanie echa / skala głosu / półton | |
| timing check / Listening paused · Resume | test rytmu / Słuchanie wstrzymane · Wznów | |
| macOS: System Settings · Privacy & Security · Screen & System Audio Recording · Microphone | Ustawienia systemowe · Prywatność i ochrona · Nagrywanie ekranu i dźwięku systemowego · Mikrofon | Mikrofon unverified |
Keep these names unchanged: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord and macOS. Don't inflect OnKey. Rephrase instead: `w aplikacji OnKey`, `aplikacja OnKey słyszy…`, `w Spotify`.

## Avoid
- Gendered forms (`-łeś/-łaś`, `gotowy/gotowa` about the user), and slashes like `zalogowany(-a)`.
- Officialese such as `Należy`, `celem`, `dokonać`, and noun chains.
- Anglicisms where Polish has a word: `skipować` → `pomiń`. Exclamation marks. English Title Case.

## Length
Polish runs about 20–30% longer than English (approximate). Prefer a short verb plus an object on buttons, and drop pronouns.

## Sources
- Microsoft Polish Localization Style Guide: https://aka.ms/polish-styleguide (3.1 gender, 4.2.8 pronouns, quotes, ellipsis)
- Spotify Premium (Polish): https://www.spotify.com/pl/premium/
- Apple Music (Polish): https://www.apple.com/pl/apple-music/
- macOS labels: https://support.apple.com/pl-pl/guide/mac-help/mchld6aa7d23/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
