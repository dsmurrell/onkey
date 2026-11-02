# Portuguese (pt) — OnKey style guide
Level: Full. Variant: Brazilian Portuguese (pt-BR). Not European Portuguese.

## Register
- **você**, always: Spotify BR ("Onde quer que você esteja", "Cancele quando quiser"), macOS pt-BR and Microsoft pt-BR all use it. Never *tu* or *o senhor*.
- Buttons, menu items and tabs use the infinitive, as macOS does ("Permitir", "Encerrar e Reabrir"): *Começar a cantar*, *Retomar*, *Configurar depois*.
- Instructions in sentences use the você imperative: "Toque uma música no Spotify ou no Apple Music", "abaixe um pouco o volume".
- Drop *você* where the verb already makes the subject clear: "Cante junto, sem dicas", not "Você canta junto".

## Punctuation and typography
- Quotes “ ” (macOS pt-BR uses them), apostrophe ’, ellipsis … with no space before. Drop most English exclamation marks.
- Sentence case for OnKey's labels ("Modo mini", "Relatar um problema"). macOS pt-BR puts some system labels in Title Case ("Ajustes do Sistema"); copy those exactly.
- Keep " · " with a space on each side. No full stop on buttons, tabs or short labels.

## Numbers, units, dates
- ICU formats numbers (decimal comma, thousands point): never hard-code separators in literal text. "35%" with no space; "35 minutos"; 24-hour clock ("21:30"); dates d/m/yyyy.

## Plurals
- CLDR pt: **one** covers **0 and 1** ("0 música", "1 música"), **many** covers exact millions (for compact numbers), and **other** covers everything else.
- Supply one and other, and check that the one form reads well with 0. If it doesn't, write a separate =0 case ("Nenhuma música").

## Terms
| English | Portuguese (BR) | Note |
|---|---|---|
| take / Blind take | take / take às cegas | Masculine (*o take*, *os takes*), the usual studio word in Brazil. "Toda música que você canta vira um take." |
| learn / learned / partly learned | aprender / aprendida / aprendida em parte | Agrees with *música* (feminine): "Aprendida · pronta para cantar". "Learn it again" → *Aprender de novo*. |
| lane | pista | Not *faixa*, which means a track. |
| notes / syllable bars | notas / barras de sílabas | |
| pitch line | linha da sua voz | |
| guide / Full guide | guia / Guia completo | "Fade the guide" → *Esmaecer o guia*. |
| score / Scoring level | pontuação / Nível de pontuação | |
| on key / On pitch | afinado / ✓ Afinado | |
| Sing · Library · History · Settings | Cantar · Biblioteca · Histórico · Ajustes | *Ajustes* matches macOS pt-BR. |
| Songs to try / starter playlist | Músicas para experimentar / playlist inicial | Use *música* for "song" throughout. |
| Report a problem | Relatar um problema | |
| Challenge a friend | Enviar um desafio | Gender-neutral. |
| Favourite (♥) | favorita / Adicionar aos favoritos | |
| Mini mode / dancer | Modo mini / dançarino | |
| Echo cancel | Cancelamento de eco | |
| voice range / semitone / timing check | extensão vocal / semitom / teste de sincronia | |
| Listening paused · Resume | Escuta pausada · Retomar | |
| What's new | Novidades | |
| macOS labels | Ajustes do Sistema · Privacidade e Segurança · Microfone · Gravação do Áudio do Sistema e da Tela | Exact macOS pt-BR strings. |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Música*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- European Portuguese: *ecrã*, *utilizador*, *ficheiro*, *tu*, and "estar a cantar" for "estar cantando".
- Formal filler: *efetuar*, *realizar*, *solicitar*, *deseja*. Write *quer* and *fazer*.
- Gendered generic nouns where a neutral form exists: *quem canta*, *pessoas*. Never "amigo(a)" or "@".
- Literal translations of the jokes. Adapt "shower singers" freely ("cantores de chuveiro" happens to work in Brazil).

## Length
Portuguese runs 20–30% longer than English. Keep buttons to 2–3 words and leave out the articles.

## Sources
- Microsoft Portuguese (Brazil) Style Guide, https://aka.ms/portuguese-brazil-styleguide (PDF: você voice, quotes, imperative/infinitive).
- https://www.spotify.com/br-pt/premium/ (você).
- macOS 26.6 system strings for pt_BR (SecurityPrivacyExtension, System Settings and Music `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
