# Korean (ko) — OnKey style guide
Level: Early. Variant: standard Korean (South Korea).

## Register
- 해요체 everywhere in sentences, statements and requests alike: “노래를 끝까지 틀어 주세요.”“다음에는 모든 음표가 보여요.” Toss's published consumer UX guide says every string in the product uses 해요체 and warns against over-honorific forms (~시겠어요?, ~께). Microsoft's Korean guide also moved requests from -십시오 to -세요 for a natural, conversational tone.
- Don't mix in 합니다체. Spotify KR and Apple Music KR marketing mix -습니다 with -세요, and Apple's Mac help uses -십시오; that is the formal register OnKey avoids.
- Buttons, tabs and labels: noun or verb-stem + 하기 noun forms, no ending: “노래 시작”, “문제 신고”, “나중에 설정”, “다시 보지 않기”, “재시작하여 업데이트”. Progress labels use 중: “위치 찾는 중…”.
- No "you": never 당신 or 귀하; Korean drops the subject (Microsoft: pronouns are seldom used).
- Honorific -시- only where natural (“노래하시는 중” is too much; “노래하는 중”).

## Punctuation and typography
- Western punctuation with normal Korean word spacing (띄어쓰기). Quotes “ ” (nested ‘ ’). Apple's Korean Mac help names settings without quotes (Apple 메뉴 > 시스템 설정을 선택한 다음…); do the same, or use ‘ ’ if a label would be ambiguous.
- No full stop on buttons, tabs or short labels; full stops on full sentences.
- Particles attach directly to Latin words and placeholders: “Spotify에서”, “OnKey가”. Choose the particle by pronunciation (OnKey = 온키 → 가/는/를; Apple Music = 뮤직 → 은/을/이). For `{song}` placeholders, rephrase so no 받침-dependent particle follows the placeholder (e.g. “노래: {song}” or “{song} 노래를”).
- Ellipsis: one … (U+2026). No !.
- Keep the " · " separator with spaces as in the source.

## Numbers, units, dates
- Counters attach to Arabic numerals: “3곡”“35분”“2번”. Thousands comma (1,000); 80% no space.
- Dates/times from ICU; literal text: 10월 16일, 오후 9:30.

## Plurals
- ICU/CLDR: `other` only. One `{n, plural, other {…}}` branch. Don't add 들 after counted nouns (“곡 3개들” is wrong; “3곡”).
- Counters: songs 곡, takes 번/회 or 테이크, minutes 분.

## Terms
| English | Korean | Note |
|---|---|---|
| take / Blind take | 테이크 / 블라인드 테이크 | Transliterated, as in Apple's Korean Logic Pro guide (테이크). In sentences “부른 기록” can read better. |
| learn / learned / partly learned / Learn it again | 익히기 / 익혔어요 / 일부만 익혔어요 / 다시 익히기 | Friendlier than 학습 (sounds like ML jargon). |
| lane | 레인 | Rhythm-game term. |
| notes / syllable bars | 음표 / 음절 막대 | |
| pitch line | 음정 선 | |
| guide / Fade the guide / Full guide | 가이드 / 가이드 점점 숨기기 / 전체 가이드 | |
| score / Scoring level | 점수, 채점 / 채점 난이도 | 채점 = karaoke scoring. |
| on key / On pitch | 음정이 맞음 / 음정 정확 | |
| Sing · Library · History · Settings | 노래 · 보관함 · 기록 · 설정 | 보관함 as in Apple Music KR (unverified). |
| Songs to try / starter playlist | 추천 곡 / 시작 플레이리스트 | |
| Report a problem / Challenge a friend | 문제 신고 / 친구에게 도전 | |
| Favourite (♥) | 즐겨찾기 | |
| Mini mode / dancer / Echo cancel | 미니 모드 / 댄서 / 에코 제거 | |
| voice range / semitone | 음역대 / 반음 | |
| Listening paused · Resume | 듣기 일시정지됨 · 다시 시작 | |
| macOS labels | 시스템 설정 / 개인정보 보호 및 보안 / 마이크 / 화면 및 시스템 오디오 녹음 | Verified on Apple's ko-KR Mac User Guide (title there says 기록, the setting itself 녹음). |
Product names stay in Latin script: OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS.

## Avoid
- 합니다체/하십시오체, 당신, 귀하, 고객님, ~시겠어요? on routine prompts; “되어요” (write “돼요”, per Toss).
- Translationese: “~하는 것이 가능합니다” → “~할 수 있어요”; possessive chains with 의.
- Exclamation marks, emoji, slang.

## Length
Korean is usually similar to or slightly shorter than English in characters, but Hangul is wider. Keep buttons to ~8 syllables; drop particles on labels (“설정 열기”).

## Sources
- Toss (Apps in Toss) consumer UX guide, 해요체 rule: https://developers-apps-in-toss.toss.im/design/consumer-ux-guide.md
- Microsoft Korean Localization Style Guide: https://aka.ms/korean-styleguide (-세요 instead of -십시오; pronouns)
- Spotify Premium Korea: https://www.spotify.com/kr-ko/premium/ ; Apple Music Korea: https://www.apple.com/kr/apple-music/
- Apple Mac User Guide (ko-KR): https://support.apple.com/ko-kr/guide/mac-help/mchld6aa7d23/26/mac and https://support.apple.com/ko-kr/guide/mac-help/mchla1b1e1fe/mac
- Logic Pro guide (ko-KR), 테이크: https://support.apple.com/ko-kr/guide/logicpro/lgcpd2ae0c0e/mac
- CLDR plural rules: https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html
