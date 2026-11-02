# Thai (th) — OnKey style guide
Level: Early. Variant: standard Thai (Thailand).

## Register
- Address the user as คุณ, and leave it out whenever the sentence is clear without it. Spotify Thailand and Apple Music Thailand both use คุณ (“คลัง iTunes ของคุณก็ยังเป็นของคุณอยู่”) and neither uses a politeness particle.
- No ครับ / ค่ะ / คะ / นะ: they mark the speaker's gender and chattiness, and Thai UI leaves them out. Neutral, polite standard Thai.
- Requests: plain verb first (“เล่นเพลงจนจบ”) or a soft ลอง… (“ลองลดเสียงเพลงลงหน่อย”). Use โปรด sparingly, as Microsoft does for errors (“โปรดลองอีกครั้ง”); avoid กรุณา (formal, bureaucratic).
- Buttons and tabs: short verb or noun phrases: “เริ่มร้องเพลง”, “รายงานปัญหา”, “ตั้งค่าภายหลัง”, “ไม่ต้องแสดงอีก”.
- If a first person is ever needed, Microsoft recommends gender-neutral ฉัน; OnKey speaking about itself uses “OnKey”.

## Punctuation and typography
- No full stop at the end of Thai sentences or labels (Microsoft: remove it). Separate clauses and sentences with a single space, not a comma.
- No commas in Thai running text except in number series (2, 3 และ 4).
- Quotes “ ” around names if needed; often unnecessary. Avoid !; ? may be omitted where the sentence already asks with ไหม / หรือไม่.
- Ellipsis: one … (U+2026) for in-progress labels: “กำลังหาตำแหน่ง…”.
- Space between Thai text and Latin words or numbers is usual: “เล่นเพลงใน Spotify”, “35 นาที”.
- Keep the " · " separator with spaces as in the source.
- Watch line breaking: Thai has no spaces between words, so avoid very long unbroken phrases in narrow cards.

## Numbers, units, dates
- Arabic numerals (not Thai digits). Thousands comma (2,000), decimal point; 80% no space.
- Dates/times come from ICU (which may use the Buddhist Era year); in literal text don't write Gregorian years. Time: 24-hour “21:30 น.”.

## Plurals
- ICU/CLDR: `other` only. One `{n, plural, other {…}}` branch.
- Number before a classifier: songs “{n} เพลง”, takes “{n} ครั้ง” or “{n} เทค”, minutes “{n} นาที”. Never add an English-style plural.

## Terms
| English | Thai | Note |
|---|---|---|
| take / Blind take | เทค / เทคแบบไม่ดูโน้ต | Transliterated (studio slang); in sentences “การร้องแต่ละครั้ง” may read better. |
| learn / learned / partly learned / Learn it again | เรียนรู้ / เรียนรู้แล้ว / เรียนรู้ได้บางส่วน / เรียนรู้ใหม่ | |
| lane | เลนโน้ต | |
| notes / syllable bars | โน้ต / แถบพยางค์ | |
| pitch line | เส้นระดับเสียง | |
| guide / Fade the guide / Full guide | ตัวช่วย / ค่อยๆ ซ่อนตัวช่วย / ตัวช่วยเต็ม | |
| score / Scoring level | คะแนน / ระดับการให้คะแนน | |
| on key / On pitch | ตรงคีย์ / เสียงตรง | |
| Sing · Library · History · Settings | ร้องเพลง · คลังเพลง · ประวัติ · การตั้งค่า | |
| Songs to try / starter playlist | เพลงแนะนำ / เพลย์ลิสต์เริ่มต้น | |
| Report a problem / Challenge a friend | รายงานปัญหา / ท้าเพื่อน | |
| Favourite (♥) | รายการโปรด | |
| Mini mode / dancer / Echo cancel | โหมดมินิ / นักเต้น / ตัดเสียงสะท้อน | |
| voice range / semitone | ช่วงเสียง / ครึ่งเสียง | |
| Listening paused · Resume | หยุดฟังชั่วคราว · ฟังต่อ | |
| macOS labels | การตั้งค่าระบบ / ความเป็นส่วนตัวและความปลอดภัย / ไมโครโฟน / การบันทึกหน้าจอและเสียงระบบ | Verified on Apple's th-TH Mac User Guide. |
Product names stay in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS.

## Avoid
- ครับ/ค่ะ, กรุณา, ท่าน, ผู้ใช้ for the reader; full stops; English commas.
- Over-transliteration when a plain Thai word exists (ช่วงเสียง, not เรนจ์เสียง).
- Teen slang (Spotify's “ฉ่ำใจ” is marketing tone; OnKey stays plain).

## Length
Thai often runs 10–30% longer than English in characters, and stacked vowels/tone marks need vertical room. Keep buttons to 2–4 words; drop คุณ and connectors on labels.

## Sources
- Microsoft Thai Localization Style Guide: https://aka.ms/thai-styleguide (คุณ, punctuation 4.1.7, ฉัน)
- Spotify Premium Thailand: https://www.spotify.com/th-th/premium/
- Apple Music Thailand: https://www.apple.com/th/apple-music/
- Apple Mac User Guide (th-TH): https://support.apple.com/th-th/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/th-th/guide/mac-help/mchla1b1e1fe/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
