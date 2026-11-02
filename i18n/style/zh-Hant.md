# Chinese, Traditional (zh-Hant) — OnKey style guide
Level: Early. Variant: Taiwan (Traditional characters, Taiwan vocabulary). Hong Kong readers understand it; see the HK note.

## Register
- Address the user as 你. Microsoft's Traditional Chinese guide still says 您 everywhere, but consumer music apps in Taiwan use 你: Apple Music Taiwan (你可盡享…), Spotify Taiwan (無論你在哪裡收聽) and Apple's own Taiwan Mac User Guide (你可以決定哪些 App…). OnKey is a friendly consumer app, so 你.
- Omit 你 when the sentence works without it.
- Buttons and tabs: short verb–object or noun phrases, no 請: 開始唱歌, 回報問題, 稍後設定, 不再顯示. 請 at most once in a sentence, for real requests.
- Tone: plain, warm spoken Mandarin as written in Taiwan; no internet slang, no hype.

## Punctuation and typography
- Full-width punctuation: ，。？：；（）、 and corner quotes 「」 (nested 『』), as Apple Taiwan and Microsoft use. No ！ unless the English has one.
- No full stop on buttons, tabs or short labels.
- Half-width space between Chinese and Latin words or Arabic numerals (Microsoft rule; Apple TW: 你的 iOS, 每月 NT$165): 在 Spotify 播放歌曲, 約 35 分鐘. No space next to full-width punctuation.
- Ellipsis: one … (U+2026) for in-progress labels as in the source: 正在尋找位置…
- Keep the " · " separator with spaces as in the source.
- macOS labels inside 「」: 打開「系統設定」.

## Numbers, units, dates
- Arabic numerals, space-separated from Chinese: 唱了 3 首歌. Thousands comma (1,000); 80% with no space.
- Dates/times from ICU; literal text: 10 月 16 日 (Microsoft TW spaces digits), 24-hour 21:30 or 晚上 9:30.

## Plurals
- ICU/CLDR: `other` only. One `{n, plural, other {…}}` branch.
- Measure words: songs 首 ({n} 首歌), takes 次 ({n} 次演唱), minutes 分鐘. No 們 on nouns.

## Terms
| English | zh-Hant (Taiwan) | Note |
|---|---|---|
| take / Blind take | 演唱 (一次演唱) / 盲唱 | Translated; 盲唱 = singing with notes hidden. |
| learn / learned / partly learned / Learn it again | 學習 / 已學會 / 部分學會 / 重新學習 | |
| lane | 音符軌道 (short: 軌道) | |
| notes / syllable bars | 音符 / 音節條 | |
| pitch line | 音高線 | |
| guide / Fade the guide / Full guide | 提示 / 逐漸隱藏提示 / 完整提示 | |
| score / Scoring level | 評分, 分數 / 評分難度 | |
| on key / On pitch | 音準 / 音準到位 | 走音 = off key (Taiwan). |
| Sing · Library · History · Settings | 唱歌 · 資料庫 · 歷史紀錄 · 設定 | 紀錄 is Taiwan's preferred form. |
| Songs to try / starter playlist | 推薦歌曲 / 入門播放清單 | Taiwan says 播放清單 (Mainland 歌单). |
| Report a problem / Challenge a friend | 回報問題 / 挑戰朋友 | 回報 is Taiwan usage for reporting a bug. |
| Favourite (♥) | 最愛 | |
| Mini mode / dancer / Echo cancel | 迷你模式 / 小舞者 / 回音消除 | |
| voice range / semitone | 音域 / 半音 | |
| Listening paused · Resume | 已暫停聆聽 · 繼續 | |
| macOS labels | 系統設定 / 隱私權與安全性 / 麥克風 / 螢幕與系統錄音 | Verified on Apple's zh-TW Mac User Guide. |
Product names stay in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS. Apple TW writes "App", not 應用程式, in Mac UI.

## Hong Kong note
Hong Kong uses the same characters and also 你 (Spotify HK: 我們會…提醒你), but some words differ: 計劃 (TW 方案), 帳戶 (TW 帳號), 網上 (TW 線上), 軟件 (TW 軟體), 視像 (TW 影片). Write standard written Chinese, never Cantonese colloquial (嘅, 啲, 咗, 唔). Use Taiwan forms; HK readers will understand them.

## Avoid
- 您; 請 on buttons; Mainland vocabulary (设置/設置, 视频/視頻, 软件/軟件, 信息/資訊 → 設定, 影片, 軟體, 訊息/資訊); 用戶 for the reader (TW: 使用者, and prefer no noun).
- Character-converted Simplified text: rewrite in Taiwan vocabulary, don't just convert.
- Half-width , . ? in Chinese text; Mainland “ ” quotes.

## Length
About 40–60% fewer characters than English, each double-width. Tabs 2–4 characters, buttons ~6.

## Sources
- Microsoft Chinese (Traditional) Localization Style Guide: https://aka.ms/chinese-traditional-styleguide (您 rule, spacing 4.2.10, quotes)
- Apple Music Taiwan: https://www.apple.com/tw/apple-music/
- Spotify Premium Taiwan: https://www.spotify.com/tw/premium/ ; Hong Kong: https://www.spotify.com/hk-zh/premium/
- Apple Mac User Guide (zh-TW): https://support.apple.com/zh-tw/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/zh-tw/guide/mac-help/mchla1b1e1fe/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
