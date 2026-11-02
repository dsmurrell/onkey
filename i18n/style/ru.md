# Russian (ru) — OnKey style guide
Level: Early. Variant: standard Russian (Russia and the CIS); no regional forms.

## Register
Address the user as **вы**, always lowercase mid-sentence (never «Вы»). Spotify (kz-ru), Apple Music support (ru-ru) and the Microsoft Russian guide all use lowercase вы. A useful side effect: past tense with вы is plural, so «вы спели» and «вы готовы» need no gender.
- Buttons and menu items: **infinitive** («Начать петь», «Сообщить о проблеме», «Больше не показывать»), as in macOS and Windows.
- Body text and hints: вы-imperative («Сделайте музыку потише или наденьте наушники»).
- Translate "your" only when it adds meaning («ваши дубли» yes, "connect your headphones" → «подключите наушники»). Never "пользователь".

## Punctuation and typography
- Quotes « », with „ “ inside. Use the typographic apostrophe ’ only in Latin product names.
- Ellipsis is one character … placed directly after the word: «Ищем ваше место в песне…».
- Use an em dash — with spaces around it, and a non-breaking space before it where possible.
- Sentence case for every label. No full stop on buttons, tabs or one-line labels. Keep " · " as the separator.
- Write ё only where meaning would be ambiguous (всё / все); otherwise е, as Spotify and Microsoft do.

## Numbers, units, dates
Decimal comma (5,5), thousands separated by a non-breaking space (10 000). Put a space before % (85 %); the Microsoft guide uses a non-breaking space. Use the 24-hour clock (14:30). Put a non-breaking space between a number and its unit or noun: «35 минут», «2 полутона».

## Plurals
CLDR categories: **one, few, many, other** (other is used for fractions). Always fill all four:
`{n, plural, one {# песня} few {# песни} many {# песен} other {# песни}}` (1 песня, 3 песни, 5/11/35 песен, 1,5 песни).
"Minutes" works the same way: «1 минуту», «3 минуты», «35 минут». Never hard-code the noun next to a number.

## Terms
| English | Russian | Note |
|---|---|---|
| take / Blind take | дубль / дубль вслепую | studio word, short |
| learn / learned / Learn it again / partly learned | выучить / выучена / Выучить заново / выучена частично | OnKey learns the song; «Выучена · можно петь» |
| lane / notes / syllable bars | дорожка / ноты / полоски слогов | |
| pitch line | линия голоса | the user's live line |
| guide / Fade the guide / Full guide | подсказка / Постепенно скрывать подсказку / Полная подсказка | |
| score / Scoring level | оценка, баллы / Строгость оценки | |
| on key / On pitch | в ноты / точно в ноту | «попадать в ноты» is the natural idiom |
| Sing · Library · History · Settings | Петь · Медиатека · История · Настройки | Медиатека matches Apple Music |
| Songs to try / starter playlist | Что спеть / стартовый плейлист | |
| Report a problem / Challenge a friend | Сообщить о проблеме / Вызов другу | |
| Favourite (♥) / What's new | В избранное, Избранное / Что нового | |
| Mini mode / dancer | Мини-режим / танцор | the dancer is a figure, so masculine is fine |
| Echo cancel / voice range / semitone | Подавление эха / диапазон голоса / полутон | |
| timing check | проверка ритма | |
| Listening paused · Resume | Прослушивание на паузе · Продолжить | |
| macOS: System Settings · Privacy & Security · Microphone · Screen & System Audio Recording | Системные настройки · Конфиденциальность и безопасность · Микрофон · Запись экрана и системного звука | verified on Apple support |
Keep these names in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord and macOS. OnKey takes masculine agreement («OnKey выучил»).

## Avoid
- Bureaucratic phrasing: «осуществить», «данный», «выполнить удаление» (write «удалить»), passive chains.
- Slang loanwords: «скипать», «юзер», «трек» used for a song (write «песня»).
- Gendered forms addressed to the user: «вы готовы» is fine; «ты готов(а)» and «(а)» suffixes are not.
- Exclamation marks, and English Title Case.

## Length
Russian runs about 15–30% longer than English (approximate). On buttons, use one infinitive plus an object, and drop «вы» and «ваш» where the meaning stays clear.

## Sources
- Microsoft Russian Localization Style Guide: https://aka.ms/russian-styleguide (pronouns 4.1.9, ellipsis, non-breaking spaces 4.1.13)
- Spotify Premium (Russian): https://www.spotify.com/kz-ru/premium/
- Apple Music support (ru-ru): https://support.apple.com/ru-ru/music
- macOS labels: https://support.apple.com/ru-ru/guide/mac-help/mchld6aa7d23/mac and https://support.apple.com/ru-ru/guide/mac-help/mchla1b1e1fe/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
