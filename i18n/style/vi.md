# Vietnamese (vi) — OnKey style guide
Level: Early. Variant: standard Vietnamese (Vietnam), neutral between North and South.

## Register
- Address the user as “bạn”, and drop it when the sentence is clear without it. Spotify Vietnam (“Chúng tôi sẽ nhắc bạn…”), Apple Music Vietnam and Apple's Vietnamese Mac User Guide all use “bạn”; Microsoft's guide says to use “bạn” and avoid gendered pronouns (anh, chị, ông, bà, cô, chú).
- Never “quý khách” or “quý vị” (shop/formal), never “mày/cậu”.
- OnKey refers to itself as “OnKey”, not “chúng tôi”, unless the English says “we”.
- In sentences, a gentle imperative with “hãy” is fine (“Hãy để bài hát phát hết”); Apple and Microsoft both use it. Use “vui lòng” only for error-type requests (“Vui lòng thử lại”).
- Buttons and tabs: bare verb phrase, no “hãy”/“vui lòng”: “Bắt đầu hát”, “Báo cáo sự cố”, “Thiết lập sau”, “Không hiện lại”.

## Punctuation and typography
- Sentence case: capitalise only the first word of a label (“Chế độ thu nhỏ”, not “Chế Độ Thu Nhỏ”), per Microsoft and Spotify VN buttons.
- No full stop on buttons, tabs or short labels; full stops on sentences. No comma before “và”/“hoặc”.
- Quotes “ ”. Ellipsis: one … (U+2026): “Đang tìm vị trí…”. Avoid !.
- Space between words and numbers as in English (“35 phút”); no extra spaces inside quotes or brackets.
- Keep the " · " separator with spaces as in the source.
- Always write full diacritics in precomposed (NFC) Unicode; never strip tones to save space.

## Numbers, units, dates
- Decimal comma, thousands period: 5,25; 2.000 (Microsoft; Spotify VN “65.000 ₫”). 80% no space.
- Dates/times from ICU; in literal text day-month order (16/10) and 24-hour (21:30).

## Plurals
- ICU/CLDR cardinal: `other` only (ordinals have `one`, unused here). One `{n, plural, other {…}}` branch.
- No plural marker after a number: “3 bài hát”, “{n} lượt hát”, “35 phút”. Don't add “các/những” after a count.

## Terms
| English | Vietnamese | Note |
|---|---|---|
| take / Blind take | lượt hát / hát không gợi ý | Translated (a sung turn); “take” is studio jargon in Vietnamese. |
| learn / learned / partly learned / Learn it again | học / đã học xong / mới học được một phần / học lại | |
| lane | dải nốt | |
| notes / syllable bars | nốt nhạc / thanh âm tiết | |
| pitch line | đường cao độ | |
| guide / Fade the guide / Full guide | gợi ý / ẩn dần gợi ý / gợi ý đầy đủ | |
| score / Scoring level | điểm, chấm điểm / mức chấm điểm | |
| on key / On pitch | đúng tông / đúng cao độ | |
| Sing · Library · History · Settings | Hát · Thư viện · Lịch sử · Cài đặt | |
| Songs to try / starter playlist | Bài hát nên thử / danh sách phát khởi đầu | |
| Report a problem / Challenge a friend | Báo cáo sự cố / Thách đấu bạn bè | |
| Favourite (♥) | Yêu thích | |
| Mini mode / dancer / Echo cancel | Chế độ thu nhỏ / vũ công / Khử tiếng vọng | |
| voice range / semitone | quãng giọng / nửa cung | |
| Listening paused · Resume | Đã tạm dừng nghe · Tiếp tục | |
| macOS labels | Cài đặt hệ thống / Quyền riêng tư & Bảo mật / Micrô / Bản ghi âm thanh hệ thống & màn hình | Verified on Apple's vi-VN Mac User Guide (Apple's own capitalisation). |
Product names stay as-is: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS.

## Avoid
- Quý khách, gendered pronouns, “người dùng” for the reader; Title Case; English-style comma before “và”.
- Sino-Vietnamese bureaucratic words where a plain word exists (“sử dụng” is fine, “tiến hành thực hiện” is not).
- Word-for-word calques: “OnKey không thể nghe bạn qua âm nhạc” → “Nhạc to quá nên OnKey không nghe rõ giọng bạn”.

## Length
Vietnamese usually runs 20–40% longer than English (many short words). Keep buttons to ~3–4 words; drop “của bạn” when obvious.

## Sources
- Microsoft Vietnamese Localization Style Guide: https://aka.ms/vietnamese-styleguide (bạn, gendered pronouns, capitalisation, numbers)
- Spotify Premium Vietnam: https://www.spotify.com/vn-vi/premium/
- Apple Music Vietnam: https://www.apple.com/vn/apple-music/
- Apple Mac User Guide (vi-VN): https://support.apple.com/vi-vn/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/vi-vn/guide/mac-help/mchla1b1e1fe/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html (checked against Node's ICU 76 `Intl.PluralRules`)
