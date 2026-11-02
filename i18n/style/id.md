# Indonesian (id) — OnKey style guide
Level: Early. Variant: standard Indonesian (Bahasa Indonesia, KBBI spelling).

## Register
Address the user as **Anda**, always capitalised, with the possessive written `lagu Anda` (not `laguanda`). Apple Music ID and the Microsoft Indonesian guide use Anda, and macOS does too, so OnKey's text will sit next to Anda dialogs. Spotify ID uses the casual **kamu/-mu**. That is a fair alternative for a fun music app, but don't mix the two. Keep the tone friendly through short, plain sentences.
- Buttons and actions: a bare imperative verb, with no `Silakan` and no `Anda`: `Mulai bernyanyi`, `Laporkan masalah`, `Jangan tampilkan lagi`, `Atur nanti`, `Mulai ulang untuk memperbarui`.
- Body text: `Anda dapat…` is fine but often unnecessary. `Kunjungi…` beats `Anda dapat mengunjungi…`, and `Lanjutkan?` beats `Anda ingin melanjutkan?` (Microsoft guide).
- Indonesian verbs have no gender, so no gender handling is needed.

## Punctuation and typography
- Quotes “ ”, with ‘ ’ inside. Ellipsis is the single character ….
- Capitalisation: sentence case for OnKey's own labels, and don't copy English Title Case. Write macOS setting names exactly as macOS does (`Pengaturan Sistem`).
- Don't put commas between stacked adjectives (Microsoft guide).
- No full stop on buttons, tabs or short labels. No exclamation marks. Keep " · " as the separator.

## Numbers, units, dates
Decimal comma (5,25), thousands dot (10.000). Write the percent sign without a space: 85%. Use the 24-hour clock (14.30 is formal style; 14:30 is acceptable in UI, but be consistent). Write `35 menit`.

## Plurals
CLDR category: **other** only, so ICU strings have one branch: `{n, plural, other {# lagu}}`.
**Never pluralise after a number**: `35 menit`, `3 lagu`, not `3 lagu-lagu`. Use reduplication (`lagu-lagu`) sparingly, and only when no number or quantifier (`semua`, `beberapa`) is present.

## Terms
| English | Indonesian | Note |
|---|---|---|
| take / Blind take | take / Take tanpa not | keep "take" (recording-studio usage). Alternative: `percobaan`. Avoid `rekaman`, which clashes with `Rekaman Layar`. Reviewers: confirm |
| learn / learned / Learn it again / partly learned | mempelajari / Sudah dipelajari / Pelajari lagi / sebagian dipelajari | `Sudah dipelajari · siap dinyanyikan` |
| lane / notes / syllable bars | jalur / not / bilah suku kata | the music sense is `not`, not `catatan` |
| pitch line | garis nada suara | |
| guide / Fade the guide / Full guide | panduan / Pudarkan panduan / Panduan penuh | |
| score / Scoring level | skor / Tingkat penilaian | |
| on key / On pitch | tepat nada / pas di nada | `fals` = off key |
| Sing · Library · History · Settings | Nyanyi · Pustaka · Riwayat · Pengaturan | `Pustaka` as in Apple Music ID (unverified) |
| Songs to try / starter playlist | Lagu untuk dicoba / playlist pembuka | |
| Report a problem / Challenge a friend | Laporkan masalah / Tantang teman | |
| Favourite (♥) / What's new | Favorit / Yang baru | |
| Mini mode / dancer | Mode mini / penari | |
| Echo cancel / voice range / semitone | Peredam gema / rentang suara / semiton | |
| timing check | cek ketepatan waktu | |
| Listening paused · Resume | Mendengarkan dijeda · Lanjutkan | |
| macOS: System Settings · Privacy & Security · Screen & System Audio Recording · Microphone | Pengaturan Sistem · Privasi & Keamanan · Rekaman Layar & Audio Sistem · Mikrofon | Mikrofon unverified |
Keep these names unchanged: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord and macOS. Everyday tech loanwords (`download`, `playlist`, `online`) are fine where Spotify ID uses them.

## Avoid
- Mixing Anda with kamu/-mu. Writing `anda` in lowercase.
- Stiff forms: `Silakan…`, `Mohon…` on every line, and `dapat dilakukan oleh Anda`.
- Overusing `Anda`: one per sentence at most, and none where the verb is clear.
- Literal word order from English, slang (`gue`, `lo`), and exclamation marks.

## Length
Indonesian runs about 20–35% longer than English (approximate), because affixed verbs are long. Drop `Anda` and `untuk` on buttons; `Mulai bernyanyi` beats `Mulai untuk bernyanyi`.

## Sources
- Microsoft Indonesian Localization Style Guide: https://aka.ms/indonesian-styleguide (voice examples with Anda, quotes, decimal comma, capitalisation)
- Apple Music (Indonesian): https://www.apple.com/id/apple-music/
- Spotify Premium (Indonesian): https://www.spotify.com/id-id/premium/
- macOS labels: https://support.apple.com/id-id/guide/mac-help/mchld6aa7d23/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
