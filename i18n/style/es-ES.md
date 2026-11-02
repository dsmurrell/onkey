# Spanish, Spain (es-ES) — OnKey style guide
Level: Full. Variant: Spanish as written in Spain (Peninsular), the way macOS es-ES and Spotify ES say it. Latin
America has its own catalog, `es` (style/es.md): start from the same terms, and change only what Spain says
differently.

## Register
- **tú**, singular, always, as macOS es-ES ("¿Quieres reiniciar…?") and Spotify ES ("Escucha música sin límites")
  do. Never *usted* or *vos*. **vosotros** only where a message really speaks to several people at once (almost
  never in OnKey).
- Buttons, menu items and tabs use the infinitive, as macOS does ("Permitir", "Salir y volver a abrir"):
  *Empezar a cantar*, *Reanudar*, *Configurar más tarde*.
- Instructions inside sentences use the tú imperative: "Pon una canción en Spotify o Apple Music", "baja un poco
  el volumen".

## Punctuation and typography
- Always open with ¿ and ¡; drop most English exclamations. Quotes « » are the RAE's first choice, but macOS es-ES
  writes “ ”: use “ ” like the rest of the Mac. Apostrophe ’, ellipsis … with no space before.
- Sentence case for every label: "Modo mini", "Informar de un problema". Nationalities, languages, months and
  weekdays are lowercase.
- Keep " · " as the separator, with a space on each side. No full stop on buttons, tabs or short labels.

## Numbers, units, dates
- ICU formats numbers (decimal comma, a space or point for thousands in Spain): never hard-code separators.
  Percent "35 %" with a non-breaking space. "35 minutos", "5 s", 24-hour clock ("21:30").

## Plurals
- CLDR: **one** (exactly 1), **many** (exact millions; compact numbers), **other**. **0 is other**: "0 canciones".
- Supply one and other. Add many only if the string can reach millions.

## Terms
Same as style/es.md, except:

| English | Spain (es-ES) | Latin America (es), for comparison |
|---|---|---|
| computer | ordenador (or *Mac*, *equipo*) | computadora |
| phone | móvil | celular / teléfono |
| headphones | auriculares | auriculares |
| click / press | hacer clic / pulsar | hacer clic / presionar |
| type in | escribir / introducir | escribir / ingresar |
| playlist | lista / playlist (Spotify ES says *playlist*) | playlist |
| Settings (OnKey's tab) | Ajustes | Ajustes |
| macOS labels | **Ajustes del Sistema** · Privacidad y seguridad · Micrófono · **Grabación de pantalla y del audio del sistema** | Configuración del Sistema · … · Grabación de audio del sistema y pantalla |
| Windows labels | Configuración · Sonido · Privacidad y seguridad | (same) |

*vale* and *coger* are fine in Spain but keep to plain, neutral words in the interface. Use *vídeo* (with the
accent), not *video*.

Keep these names as they are: OnKey, Spotify, Apple Music (Apple's own app is *Música*), YouTube, Shazam, LRCLIB,
GitHub, Discord, Mac, macOS.

## Avoid
- Latin American words where Spain has its own (*computadora*, *celular*, *agarrar*, *jalar*, *carro*).
- Calques ("aplicar" for "apply for", *realizar* everywhere), anglicisms beyond *playlist*, and slang.
- Generic masculine nouns where a neutral form exists: *quien canta* or *tus amistades*, never "amigo/a" or "@".

## Length
Spanish runs 20–30% longer than English. On buttons, drop the articles and choose short verbs (*Reanudar*, not
*Reanudar la escucha*).

## Sources
- macOS 26.6 system strings for es (Spain): System Settings, SecurityPrivacyExtension `.loctable` files (as noted in
  style/es.md: *Ajustes del Sistema*, *Grabación de pantalla y del audio del sistema*).
- https://www.spotify.com/es/premium/ (tú).
- RAE, Ortografía de la lengua española (2010): quotes, ¿ ¡, numbers.
- CLDR plural rules, https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
