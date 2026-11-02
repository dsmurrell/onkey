# Hindi (hi) — OnKey style guide
Level: Early. Variant: standard Hindi (India), Devanagari script. Never use romanised Hindi.

## Register
Address the user as **आप**, with honorific verb forms (करें, गाएँ, चाहें). Spotify India's Hindi pages and the Microsoft Hindi guide use आप. Never use तुम or तू.
- Buttons and actions: the polite imperative ending in -एँ/-ें: `गाना शुरू करें`, `समस्या की रिपोर्ट करें`, `दोबारा न दिखाएँ`, `बाद में सेट अप करें`, `अपडेट के लिए रीस्टार्ट करें`.
- Use the reflexive possessive after आप: `आप अपने गाने…`, never `आप आपके…` (Microsoft guide, "known issue").
- **Gender:** perfective verbs agree with the subject's gender (`आप थक गए/गई हैं`). Avoid that construction with आप as the subject. Use the ergative `आपने गाया`, a noun (`आपका स्कोर`), or make the song or OnKey the subject.
- Use everyday spoken Hindi, the kind Spotify uses. Write common English tech and music words in Devanagari (प्लेलिस्ट, डाउनलोड, स्कोर, सेटिंग्ज़, ऑडियो). Avoid Sanskritised coinages nobody says (e.g. अभिलेखन for "recording"). Don't mix Latin script into sentences except for product names.

## Punctuation and typography
- End sentences with the English full stop **.**, not the पूर्ण विराम ।. Spotify Hindi and the Microsoft guide both do this.
- Use the nukta where the loanword has it: फ़, ज़, ऑ (ऑफ़लाइन, म्यूज़िक, सेटिंग्ज़).
- Quotes “ ”. Ellipsis is the single character …. Keep " · " as the separator.
- No full stop on buttons, tabs or short labels. No exclamation marks.

## Numbers, units, dates
Use Western digits 0–9, not Devanagari digits. Write the percent sign without a space: 85% (Microsoft guide). Large numbers use Indian grouping (1,00,000) if they ever appear in literal text. Use `35 मिनट` (मिनट does not change in the plural) and `14:30` or `2:30 PM` as the system shows it.

## Plurals
CLDR categories: **one** (0 and 1) and **other**. Note that **0 is "one"** in Hindi:
`{n, plural, =0 {कोई गाना नहीं} one {# गाना} other {# गाने}}`. Use an `=0` branch when "0 songs" should read as "no songs".
Masculine nouns in -आ change (गाना/गाने); other masculine nouns don’t (मिनट, टेक, सेमीटोन); feminine nouns do (1 लाइन / 2 लाइनें).

## Terms
| English | Hindi | Note |
|---|---|---|
| take / Blind take | टेक / ब्लाइंड टेक | known from film and music studios |
| learn / learned / Learn it again / partly learned | सीखना / सीख लिया / फिर से सीखें / कुछ हिस्सा सीखा | `सीख लिया · गाने के लिए तैयार` |
| lane / notes / syllable bars | लेन / सुर / सुर बार | सुर is the natural word for sung notes |
| pitch line | पिच लाइन | the user's live voice line |
| guide / Fade the guide / Full guide | गाइड / गाइड धीरे-धीरे हटाएँ / पूरा गाइड | |
| score / Scoring level | स्कोर / स्कोरिंग लेवल | |
| on key / On pitch | सुर में / सही सुर पर | `सुर में गाना` = sing on key; `बेसुरा` = off key |
| Sing · Library · History · Settings | गाएँ · लाइब्रेरी · इतिहास · सेटिंग्ज़ | |
| Songs to try / starter playlist | आज़माने के लिए गाने / शुरुआती प्लेलिस्ट | |
| Report a problem / Challenge a friend | समस्या की रिपोर्ट करें / दोस्त को चैलेंज करें | |
| Favourite (♥) / What's new | पसंदीदा / नया क्या है | |
| Mini mode / dancer | मिनी मोड / डांसर | |
| Echo cancel / voice range / semitone | इको कैंसिल / आवाज़ की रेंज / सेमीटोन | |
| timing check | टाइमिंग जाँच | |
| Listening paused · Resume | सुनना रुका है · फिर से शुरू करें | |
| macOS: System Settings · Privacy & Security · Screen & System Audio Recording · Microphone | सिस्टम सेटिंग्ज़ · गोपनीयता और सुरक्षा · स्क्रीन और सिस्टम ऑडियो रिकॉर्डिंग · माइक्रोफ़ोन | माइक्रोफ़ोन unverified |
Keep these names in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord and macOS. The same goes for Spotify Hindi (`Premium`) and Apple support (`Apple मेनू`).

## Avoid
- तुम or तू, and mixing आप with तुम verb forms.
- Heavy Sanskritised or bureaucratic Hindi (कृपया on every line, `संसाधित करें`, `निष्पादित करें`). Also avoid Latin-script Hinglish.
- Gendered verb forms about the user (see Register). The computer speaking in the first person singular (`मैं…`).
- Literal idioms (`shower singers` → translate the meaning: `बाथरूम सिंगर` is natural).

## Length
Hindi runs about 20–40% longer than English (approximate), and Devanagari needs more vertical space. Prefer short noun or verb pairs on buttons (`गाना शुरू करें`), and drop आप where the verb form already implies it.

## Sources
- Microsoft Hindi Localization Style Guide: https://aka.ms/hindi-styleguide (pronouns 4.1.11, full stop, percent, error-message voice)
- Spotify Premium (Hindi): https://www.spotify.com/in-hi/premium/ (आप, loanwords in Devanagari, "." full stop)
- macOS labels: https://support.apple.com/hi-in/guide/mac-help/mchld6aa7d23/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
- Apple Music India (https://www.apple.com/in/apple-music/) is English-only, so it was not used for register.
