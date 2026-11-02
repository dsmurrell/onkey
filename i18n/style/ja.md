# Japanese (ja) — OnKey style guide
Level: Early. Variant: standard Japanese (Japan).

## Register
- Sentences in です・ます (polite, not keigo): 「曲を最後まで再生してください。」「次は全部の音符が見えます。」 Spotify JP and Apple Music JP write body copy in です・ます; Microsoft's Japanese guide asks for everyday words over stiff written ones (選ぶ over 選択する in sentences, お勧めします over 推奨します).
- Buttons, tabs, labels and status chips: noun or plain dictionary form, no です/ます, no 〜してください: 「歌いはじめる」「問題を報告」「再起動してアップデート」「あとで設定」「今後表示しない」. Spotify JP buttons do the same (「すべてのプランを見る」「1ヶ月間￥0で体験」).
- Drop "you". Japanese omits the subject; never あなた (Microsoft: often sounds impolite). If unavoidable, rephrase around the action.
- Light warmth is fine (〜しましょう for gentle suggestions), but no hype and no casual タメ口 in sentences.

## Punctuation and typography
- Japanese punctuation full-width: 、。「」（）？. Nested quotes 『』. Never English "…" quotes around Japanese text.
- No full stop on buttons, tabs or short labels; use 。 on full sentences.
- No space between Japanese and Latin words or numbers: 「Spotifyで曲を再生」「35分」. This follows Apple's own Japanese pages (「Macの画面収録」, 「Apple Musicは…」), which suits a Mac app; Microsoft's guide differs (half-width space), don't follow it here.
- Ellipsis: one "…" (U+2026), as in the source: 「曲の位置を探しています…」. No ！ (Apple JP pages avoid it); ？ full-width.
- Keep the " · " separator with half-width spaces as in the source (do not swap for ・).
- Katakana compounds from English: no space (「ミニモード」), long vowel ー per 外来語の表記 (「ダンサー」, 「ライブラリ」 is Apple's established form without ー).

## Numbers, units, dates
- Arabic numerals, half-width: 「35分ほど」「3曲」. Thousands with a comma (1,000). "%" directly after the number (80%).
- Dates and times come from the app's ICU formatting; in literal text use 「10月16日」 and 24-hour 「21:30」.

## Plurals
- ICU/CLDR: `other` only. Write one `{n, plural, other {…}}` branch; never add `one`.
- Use counters: songs 「{n}曲」, takes 「{n}回」 or 「{n}テイク」, minutes 「{n}分」. Don't make "1曲" a special case unless English does for meaning.

## Terms
| English | Japanese | Note |
|---|---|---|
| take / Blind take | テイク / ブラインドテイク | Katakana; Apple's Japanese Logic Pro guide uses テイク. In sentences 「歌った記録」 may read better. |
| learn / learned / partly learned / Learn it again | 覚える / 覚えました / 一部だけ覚えました / もう一度覚える | Friendly verb, not 学習 (sounds like ML jargon). |
| lane | レーン | Familiar from rhythm games. |
| notes / syllable bars | 音符 / 音程バー | 音程バー is the karaoke-scoring term singers know. |
| pitch line | 音程ライン | The user's live voice line. |
| guide / Fade the guide / Full guide | ガイド / ガイドを薄くする / フルガイド | |
| score / Scoring level | スコア・採点 / 採点の厳しさ | 採点 = karaoke scoring. |
| on key / On pitch | 音程が合っている / 音程どおり | |
| Sing · Library · History · Settings | 歌う · ライブラリ · 履歴 · 設定 | Tabs. ライブラリ as in Apple Music JP. |
| Songs to try / starter playlist | おすすめの曲 / はじめてのプレイリスト | |
| Report a problem / Challenge a friend | 問題を報告 / 友だちに挑戦 | |
| Favourite (♥) | お気に入り | |
| Mini mode / dancer / Echo cancel | ミニモード / ダンサー / エコーキャンセル | Katakana. |
| voice range / semitone | 音域 / 半音 | Translated. |
| Listening paused · Resume | 聞き取りを一時停止中 · 再開 | |
| macOS labels | システム設定 / プライバシーとセキュリティ / マイク / 画面収録とシステムオーディオ録音 | Verified on Apple's Japanese Mac User Guide; wrap in 「」 in sentences. |
Product names stay in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS.

## Avoid
- あなた, お客様, ユーザー for the reader; 〜してください on buttons; ！; stiff 漢語 (実行する, 選択する) in friendly sentences.
- Literal English order ("OnKeyはあなたが聞こえません" → 「音楽の音が大きくて声が聞き取れません。」).
- Excess katakana when a plain Japanese word exists (音域 not ボイスレンジ).

## Length
Japanese is usually 30–50% fewer characters than English, but each is full-width, so visual width is similar. Keep buttons to ~8 characters; cut particles (「設定を開く」 not 「設定を開いてください」).

## Sources
- Microsoft Japanese Localization Style Guide: https://aka.ms/japanese-styleguide (voice, pronouns, symbols & spaces)
- Spotify Premium Japan: https://www.spotify.com/jp/premium/
- Apple Music Japan: https://www.apple.com/jp/apple-music/
- Apple Mac User Guide (ja): https://support.apple.com/ja-jp/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/ja-jp/guide/mac-help/mchla1b1e1fe/mac
- Logic Pro guide (ja), テイク: https://support.apple.com/ja-jp/guide/logicpro/lgcpd2ae0c0e/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
