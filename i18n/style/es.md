# Spanish, Latin America (es) — OnKey style guide
Level: Full. Variant: neutral Latin American Spanish (es-419), readable across Latin America and by Spanish speakers in the US. No vosotros, no voseo. Spain has its own catalog, `es-ES` (style/es-ES.md): keep the two apart, don't write for Spain here.

## Register
- **tú**, singular, always: Spotify (/mx and /es: "Escucha Spotify donde quieras"), macOS es and es-419 ("¿Quieres reiniciar…?") and Microsoft's neutral voice all use it. Never *usted*, *vosotros* (Spotify ES slips once: "vuestros gustos") or *vos* (*querés*).
- Buttons, menu items and tabs use the infinitive, as macOS does ("Permitir", "Salir y volver a abrir"): *Empezar a cantar*, *Reanudar*, *Configurar más tarde*.
- Instructions inside sentences use the tú imperative: "Reproduce una canción en Spotify o Apple Music", "baja un poco el volumen".

## Punctuation and typography
- Always open with ¿ and ¡; drop most English exclamations (Microsoft does too). Quotes “ ” (as macOS es), apostrophe ’, ellipsis … with no space before.
- Sentence case for every label: "Modo mini", "Informar de un problema". Nationalities, languages, months and weekdays are lowercase.
- Keep " · " as the separator, with a space on each side. No full stop on buttons, tabs or short labels.

## Numbers, units, dates
- ICU formats numbers (decimal comma in Spain, decimal point in Mexico and much of Latin America): never hard-code separators. Percent: "35 %" with a non-breaking space is the RAE norm (unverified); otherwise follow ICU. "35 minutos", "5 s", 24-hour clock ("21:30").

## Plurals
- CLDR: **one** (exactly 1), **many** (exact millions, such as 1 000 000; used for compact numbers), **other**. **0 is other**: "0 canciones".
- Supply one and other. Add many only if the string can reach millions.

## Terms
| English | Spanish | Note |
|---|---|---|
| take / Blind take | toma / toma a ciegas | Studio word, feminine: "Cada canción que cantas se convierte en una toma." |
| learn / learned / partly learned | aprender / aprendida / aprendida en parte | Agrees with *canción* (feminine): "Aprendida · lista para cantar". "Learn it again" → *Volver a aprenderla*. |
| lane | carril | Not *pista* ("no hints" = *sin pistas*). |
| notes / syllable bars | notas / barras de sílabas | |
| pitch line | línea de tu voz | |
| guide / Full guide | guía / Guía completa | "Fade the guide" → *Atenuar la guía*. |
| score / Scoring level | puntuación / Nivel de puntuación | *Puntuación* works everywhere; avoid *puntaje* (Latin America only). |
| on key / On pitch | afinado / ✓ Afinado | |
| Sing · Library · History · Settings | Cantar · Biblioteca · Historial · Ajustes | |
| Songs to try / starter playlist | Canciones para probar / playlist para empezar | Spotify ES and MX say *playlist*. |
| Report a problem | Informar de un problema | |
| Challenge a friend | Enviar un reto | Gender-neutral; avoids *un amigo/una amiga*. |
| Favourite (♥) | favorita / Añadir a favoritos | |
| Mini mode / dancer | Modo mini / bailarín | |
| Echo cancel | Cancelación de eco | |
| voice range / semitone / timing check | rango vocal / semitono / prueba de sincronía | |
| Listening paused · Resume | Escucha en pausa · Reanudar | |
| What's new | Novedades | |
| macOS labels | Configuración del Sistema · Privacidad y seguridad · Micrófono · Grabación de audio del sistema y pantalla | macOS es-419 labels, as Latin American Macs show them. (Spain's differ: see style/es-ES.md.) |

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Música*), YouTube, Shazam, LRCLIB, GitHub, Discord, Mac, macOS.

## Avoid
- Words that change between regions. Use the safe choice: *computadora/ordenador* → *Mac* or *equipo*; *celular/móvil* → *teléfono*; *ingresar/introducir* → *escribir*; *pulsar/presionar* → *hacer clic*/*tocar*; *audífonos* → *auriculares*; *agregar* → *añadir*; never *vale* or *coger*.
- Calques ("aplicar" for "apply for", *realizar* everywhere), anglicisms beyond *playlist*, and slang.
- Generic masculine nouns where a neutral form exists: *quien canta* or *tus amistades*, never "amigo/a" or "@".

## Length
Spanish runs 20–30% longer than English. On buttons, drop the articles and choose short verbs (*Reanudar*, not *Reanudar la escucha*).

## Sources
- Microsoft Spanish (Neutral) Style Guide, https://aka.ms/spanish-neutral-styleguide (PDF: neutral Spanish, *computadora/ordenador*, tú, quotes, capitalisation).
- https://www.spotify.com/mx/premium/ and https://www.spotify.com/es/premium/ (tú).
- macOS 26.6 system strings for es and es-419 (SecurityPrivacyExtension and System Settings `.loctable` files, read locally).
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
