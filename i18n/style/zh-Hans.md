# Chinese, Simplified (zh-Hans) — OnKey style guide
Level: Early. Variant: Mainland China, Simplified characters, Mainland vocabulary.

## Register
- Address the user as 你, never 您. Apple Music's Chinese page (apple.com.cn) uses 你 throughout, and Microsoft's Simplified Chinese guide says: if a pronoun is needed, use 你 instead of 您.
- Omit 你 when the sentence is clear without it (Microsoft: omit pronouns for fluent text).
- Buttons and tabs: short verb–object or noun phrases, no 请: 开始唱歌, 报告问题, 稍后设置, 不再显示. In sentences use 请 at most once, only where English says "please" or the action is a real request.
- Tone: plain spoken Mandarin, friendly, no 网络用语 or slang, no hype words (超赞, 极致).

## Punctuation and typography
- Full-width Chinese punctuation in Chinese text: ，。？：；（）、 and quotes “ ” (nested ‘ ’). No ！ unless the English has one (it rarely does).
- No full stop on buttons, tabs or short labels.
- Half-width space between Chinese characters and Latin words or Arabic numerals: 在 Spotify 中播放歌曲, 约 35 分钟. No space next to full-width punctuation (“系统设置”中). Both Apple CN (你的 iTunes 资料库, 3 个月内) and Microsoft do this.
- Ellipsis: one … (U+2026) for in-progress labels, as in the source: 正在查找位置…. Don't use three periods.
- Keep the " · " separator with spaces as in the source.
- macOS labels in sentences go inside “ ”: 打开“系统设置”, as Apple's Chinese guides write it.

## Numbers, units, dates
- Arabic numerals with a space either side when next to Chinese: 唱了 3 首歌. Thousands separator comma (1,000); percent directly after the number (80%).
- Dates/times come from ICU; in literal text: 10 月 16 日 (same digit spacing as 3 个月内), 24-hour 21:30.

## Plurals
- ICU/CLDR: `other` only. One `{n, plural, other {…}}` branch.
- Measure words: songs 首 ({n} 首歌), takes 次 ({n} 次演唱), minutes 分钟. Never add 们 for plural nouns (歌曲们).

## Terms
| English | zh-Hans | Note |
|---|---|---|
| take / Blind take | 演唱 (一次演唱) / 盲唱 | Translated. Apple's Logic Pro (zh-CN) uses 分段, too technical here. 盲唱 = sing with notes hidden (like 盲打); add 不看提示 if the context needs it. |
| learn / learned / partly learned / Learn it again | 学习 / 已学会 / 部分学会 / 重新学习 | |
| lane | 音符轨道 (short: 轨道) | Rhythm-game word players know. |
| notes / syllable bars | 音符 / 音节条 | |
| pitch line | 音高线 | |
| guide / Fade the guide / Full guide | 提示 / 逐渐隐藏提示 / 完整提示 | |
| score / Scoring level | 评分, 得分 / 评分难度 | KTV scoring vocabulary. |
| on key / On pitch | 音准 / 音准到位 | 跑调 = off key. |
| Sing · Library · History · Settings | 唱歌 · 资料库 · 历史记录 · 设置 | 资料库 as in Apple Music CN. |
| Songs to try / starter playlist | 推荐歌曲 / 入门歌单 | 歌单 is the Mainland word for playlist. |
| Report a problem / Challenge a friend | 报告问题 / 挑战好友 | |
| Favourite (♥) | 收藏 | |
| Mini mode / dancer / Echo cancel | 迷你模式 / 小舞者 / 回声消除 | |
| voice range / semitone | 音域 / 半音 | |
| Listening paused · Resume | 已暂停聆听 · 继续 | |
| macOS labels | 系统设置 / 隐私与安全性 / 麦克风 / 录屏与系统录音 | Verified on Apple's zh-CN Mac User Guide. |
Product names stay in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS.

## Avoid
- 您; 请 on buttons; 用户 for the reader; Taiwan vocabulary (設定, 影片, 軟體, 播放清單 → 设置, 视频, 软件, 歌单).
- Calques: 让一首歌播放到最后 → 把歌完整播放一遍; 成为一个 take → 留下一次演唱记录.
- Half-width , . ? in Chinese text; missing spaces around Latin words and numbers.

## Length
Chinese usually needs 40–60% fewer characters than English but each is double-width. Keep tabs to 2–4 characters and buttons to ~6.

## Sources
- Microsoft Chinese (Simplified) Localization Style Guide: https://aka.ms/chinese-simplified-styleguide (pronouns, punctuation, ellipsis)
- Apple Music China: https://www.apple.com.cn/apple-music/
- Apple Mac User Guide (zh-CN): https://support.apple.com/zh-cn/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/zh-cn/guide/mac-help/mchla1b1e1fe/mac
- Logic Pro guide (zh-CN): https://support.apple.com/zh-cn/guide/logicpro/lgcpd2ae0c0e/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
