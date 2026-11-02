# Turkish (tr) — OnKey style guide
Level: Early. Variant: standard Turkish (Türkiye).

## Register
Address the user with **siz**: `Müziği biraz kısın veya kulaklık kullanın`. The Microsoft Turkish guide sets siz as the default and keeps sen for young audiences and marketing taglines. Apple Music TR uses siz (`Hemen kaydolun`). Spotify TR uses sen, but OnKey sits next to macOS dialogs, which use siz. Keep the tone warm through short sentences, not through sen.
- **Buttons, tabs and menu items: the bare verb stem**, as in macOS and Windows: `Kaydet`, `Sorun bildir`, `Söylemeye başla`, `Bir daha gösterme`, `Sonra ayarla`, `Güncellemek için yeniden başlat`. This is the normal UI convention and doesn't break the siz register.
- Body text and hints: siz forms (`-in/-ın`, `-ebilirsiniz`). Don't mix sen and siz within one screen.
- Leave out `siz/sizin` where the verb ending already shows the person.

## Punctuation and typography
- Use the ’ apostrophe before suffixes on proper names. Pick the suffix by how the name is pronounced: `OnKey’i, OnKey’in, OnKey’e` (said "on-ki"), `Spotify’da`, `Apple Music’te`, `YouTube’da`, `Shazam’la`.
- Quotes: use “ ” to match the source. The Microsoft guide prefers straight quotes; either is acceptable, but be consistent.
- Ellipsis is the single character …. Keep " · " as the separator.
- Capitalisation: sentence case. Write macOS setting names exactly as macOS does (they use Title Case, e.g. `Sistem Ayarları`).
- Dotted and dotless i: i/İ and ı/I are different letters. Never change case automatically; write `İptal`, not `Iptal`.
- No full stop on buttons, tabs or short labels. No exclamation marks.

## Numbers, units, dates
Decimal comma (5,5), thousands dot (10.000). **Percent sign before the number with no space: %85.** Use the 24-hour clock (14:30) and `35 dakika`.

## Plurals
CLDR categories: **one, other**. But **a noun stays singular after a number**: `35 dakika`, `3 şarkı`, never `3 şarkılar`. ICU still needs both branches, and they are usually identical:
`{n, plural, one {# şarkı} other {# şarkı}}`. Add the plural suffix only when no number is given (`Şarkılar`). Don't use -ler where Turkish would use the singular (`Parametre ekle`).

## Terms
| English | Turkish | Note |
|---|---|---|
| take / Blind take | deneme / Notasız deneme | "attempt"; avoid `kayıt`, which clashes with screen recording |
| learn / learned / Learn it again / partly learned | öğrenmek / Öğrenildi / Yeniden öğren / kısmen öğrenildi | `Öğrenildi · söylemeye hazır` |
| lane / notes / syllable bars | şerit / notalar / hece çubukları | |
| pitch line | ses çizgisi | |
| guide / Fade the guide / Full guide | rehber / Rehberi azalt / Tam rehber | |
| score / Scoring level | puan / Puanlama düzeyi | |
| on key / On pitch | doğru tonda / tam perdesinde | the opposite is `detone` |
| Sing · Library · History · Settings | Söyle · Arşiv · Geçmiş · Ayarlar | `Arşiv` as in Apple Music TR (unverified) |
| Songs to try / starter playlist | Denenecek şarkılar / başlangıç çalma listesi | |
| Report a problem / Challenge a friend | Sorun bildir / Arkadaşına meydan oku | on a button the sen possessive reads naturally; in body text use `arkadaşınıza`
| Favourite (♥) / What's new | Favori / Yenilikler | |
| Mini mode / dancer | Mini mod / dansçı | |
| Echo cancel / voice range / semitone | Yankı engelleme / ses aralığı / yarım ton | |
| timing check | zamanlama kontrolü | |
| Listening paused · Resume | Dinleme duraklatıldı · Devam et | |
| macOS: System Settings · Privacy & Security · Screen & System Audio Recording · Microphone | Sistem Ayarları · Gizlilik ve Güvenlik · Ekran ve Sistem Sesi Kaydı · Mikrofon | Mikrofon unverified |
Keep these names unchanged: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord and macOS.

## Avoid
- Mixing sen and siz, and stiff officialese (`gerçekleştiriniz`, `-mektedir` chains). Write `Şarkıyı öğreniyor…`, not `Şarkı öğrenilmektedir`.
- Calques of English word order. Turkish is verb-final, so restructure rather than translating word by word.
- Over-using pronouns. Exclamation marks.

## Length
Turkish runs about 10–30% longer than English (approximate), and agglutinated words get long. On buttons, use the bare stem plus at most one object.

## Sources
- Microsoft Turkish Localization Style Guide: https://aka.ms/turkish-styleguide (4.1.13 pronouns: siz default; quotes; plural formation; % placement)
- Apple Music (Turkish): https://www.apple.com/tr/apple-music/
- Spotify Premium (Turkish): https://www.spotify.com/tr/premium/
- macOS labels: https://support.apple.com/tr-tr/guide/mac-help/mchld6aa7d23/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
