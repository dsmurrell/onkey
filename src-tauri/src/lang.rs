//! [lyrics-ml] Which language and script a song's lyrics are in, from the text alone (no network), and syllables
//! for the Latin-script languages other than English. The word timer (wav2vec2, English letters) and the syllable
//! bars work on Latin letters: a song in another script gets no bars (its LRCLIB lines are shown as they are, see
//! `alignable`), and a Spanish, Portuguese, Italian, French or German song gets its syllables counted and split by
//! that language's spelling (vowel nuclei and onset clusters) rather than the English dictionary, which has
//! "come" or "liebe" as one syllable and drops every accented letter. English keeps syllables.rs's path, unchanged.
use std::collections::HashSet;
use std::sync::OnceLock;

/// The writing system of a letter (for lyric text: which songs the English word timer can hear at all).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Script { Latin, Han, Kana, Hangul, Devanagari, Thai, Cyrillic, Arabic, Hebrew, Greek, Other }

/// A letter's script; None for anything that isn't a letter (digits, punctuation, spaces, symbols).
pub fn script_of(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    Some(match c as u32 {
        0x41..=0x5A | 0x61..=0x7A | 0xAA | 0xBA | 0xC0..=0x24F | 0x1E00..=0x1EFF => Script::Latin,
        0x3005 | 0x3007 | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F => Script::Han,
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Script::Hangul,
        0x900..=0x97F => Script::Devanagari,
        0xE00..=0xE7F => Script::Thai,
        0x400..=0x52F => Script::Cyrillic,
        0x600..=0x6FF | 0x750..=0x77F => Script::Arabic,
        0x590..=0x5FF => Script::Hebrew,
        0x370..=0x3FF => Script::Greek,
        _ => Script::Other,
    })
}

/// The script most of a word's letters are in; None for a word with no letters.
pub fn word_script(w: &str) -> Option<Script> {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for s in w.chars().filter_map(script_of) {
        match counts.iter_mut().find(|c| c.0 == s) {
            Some(c) => c.1 += 1,
            None => counts.push((s, 1)),
        }
    }
    counts.into_iter().max_by_key(|c| c.1).map(|c| c.0)
}

/// The script most of the lyrics' letters are in (Latin for an empty text).
pub fn song_script(lines: &[(f64, String)]) -> Script {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for s in lines.iter().flat_map(|(_, l)| l.chars()).filter_map(script_of) {
        match counts.iter_mut().find(|c| c.0 == s) {
            Some(c) => c.1 += 1,
            None => counts.push((s, 1)),
        }
    }
    counts.into_iter().max_by_key(|c| c.1).map_or(Script::Latin, |c| c.0)
}

/// Whether the word timer can hear these lyrics at all: at least 20 Latin letters, and a tenth of the letters or
/// more. Below that (a Mandarin, Japanese, Korean, Hindi, Thai or Russian song, even with an English word or two)
/// the whole-song word timing, the per-line one and the live line-up are skipped: the English letters it could
/// align are too few to place anything, and the model pass took a minute of CPU for nothing. The LRCLIB line
/// times are kept and the lines are shown without bars. A mixed song (a Korean song with English hooks, a
/// Spanish song with an English chorus) is still aligned by its Latin words.
pub fn alignable(lines: &[(f64, String)]) -> bool {
    let (mut latin, mut all) = (0usize, 0usize);
    for s in lines.iter().flat_map(|(_, l)| l.chars()).filter_map(script_of) {
        all += 1;
        latin += usize::from(s == Script::Latin);
    }
    latin >= 20 && latin * 10 >= all
}

/// A Latin-script language the syllables are counted for. Anything else in Latin letters is English (today's path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang { En, Es, Pt, It, Fr, De }

impl Lang {
    pub fn code(self) -> &'static str {
        match self { Lang::En => "en", Lang::Es => "es", Lang::Pt => "pt", Lang::It => "it", Lang::Fr => "fr", Lang::De => "de" }
    }
}

const LANGS: [Lang; 6] = [Lang::En, Lang::Es, Lang::Pt, Lang::It, Lang::Fr, Lang::De];

// Function words and the commonest lyric words per language (lower case, without accents dropped). Words shared
// by several languages ("no", "me", "de", "que") are in each; pure vocables ("la", "na", "oh") are in none, so a
// "la la la" line says nothing about the language. Apostrophe forms ("j'", "l'") are the token up to and with the
// apostrophe.
const WORDS_EN: &str = "the and you i to of in it me my that is your on we be for all with love know like but not this \
    what so just can when don't got yeah baby never now do are was no up down if they she he say want let come go get \
    one take make time see feel way can't ain't gonna wanna 'cause there here out about will would could have has had \
    been were am our us him her them from by at or as an than then too very more some any every night day heart life \
    world i'm it's you're i'll i've we're that's don't didn't won't nothing something everything always been girl boy \
    tonight right away again still only ever back cause into through over under where why how who because ooh";
const WORDS_ES: &str = "que de el y no te mi tu por con es para como pero yo está más un una lo se me sin amor corazón \
    siempre nunca cuando donde quiero vida todo nada porque aquí así sí ya muy soy eres tengo tienes noche cómo qué \
    los las del al en su sus este esta esto ese esa eso ella él nosotros ellos hay fue ser estar hoy mañana mundo \
    bien mal otra otro vez tiempo solo sola dime dame vamos vente hasta desde entre sobre contigo conmigo nunca jamás \
    quiere quieres puedo puedes sabes sé voy vas ven dentro fuera cielo alma besos ojos manos nadie algo alguien \
    aunque mientras después antes también tampoco tan tanto todos todas bailar cantar sentir tus mis";
const WORDS_PT: &str = "que não de você eu é um uma com para meu minha mais coração amor quando tudo nada sempre muito \
    também são está ser ela ele nós vou até só já aqui assim porque tem tenho pra se mas ou em no do da dos das \
    pelo pela nos os as o e seu sua seus suas esse essa isso este esta isto aquele aquela foi sou vai vem quero \
    queria saudade vida mundo noite dia bem mal outra outro vez tempo então ainda nunca jamais sem sobre entre \
    comigo contigo depois antes agora onde como quem qual nem sim gente coisa coisas ninguém alguém tão tanto todos \
    todas cantar sentir fazer dizer ver saber posso pode sei sabe teu tua teus tuas por sem mim ti";
const WORDS_IT: &str = "che di non il e un una per con mi ti ci si lo io tu sei sono come quando sempre mai amore \
    cuore vita tutto niente perché più qui così anche ma se ho hai del della nel nella al alla dove cosa questo questa \
    voglio noi voi gli le questi queste quello quella lui lei loro sua suo suoi sue mia mio mie miei tua tuo tue tuoi \
    nostra nostro vostra vostro essere fare dire vedere sapere senza dentro fuori sopra sotto ancora già poi ora \
    adesso ieri oggi domani notte giorno mondo cielo vero bene male altra altro volta tempo solo sola insieme \
    nessuno qualcosa qualcuno tanto tutti tutte cantare sentire posso puoi può te me sé l' d' c' un' all' dell' nell' quest' com'";
const WORDS_FR: &str = "je tu il elle nous vous ils elles le les un une des de du et est que qui ne pas mais mon ma \
    mes ton ta tes dans sur pour avec sans comme quand toujours jamais amour cœur vie tout rien pourquoi plus ici \
    aussi si ce cette où moi toi lui on y en au aux très bien oui non à été être faire veux suis es sont avons avez \
    ont ai as a j'ai c'est j' c' l' d' n' s' t' m' qu' son sa ses leur leurs notre nos votre vos encore déjà puis \
    alors maintenant hier aujourd'hui demain nuit jour monde ciel vrai mal autre fois temps seul seule ensemble \
    personne quelque chose quelqu'un tant tous toutes chanter sentir peux peut sais sait rêve yeux mains";
const WORDS_DE: &str = "ich du er sie es wir ihr der die das ein eine einen und ist nicht nein ja mein meine dein deine \
    in auf für mit ohne wie wenn immer nie niemals liebe herz leben alles nichts warum mehr hier auch aber oder dass \
    den dem des zu von bei nach über noch nur so was wer wo hab habe hast bin bist sind will kann muss mich dich uns \
    euch zum zur im am vom sich schon mal doch dann denn weil wird werden war waren hat haben kein keine einer eines \
    einem diese dieser dieses jetzt heute morgen gestern nacht tag welt himmel wahr allein zusammen niemand etwas \
    jemand ganz alle singen fühlen kannst darf wollen sehen sagen weiß weißt gibt bleib bleibt komm kommt geh geht";

fn word_set(lang: Lang) -> &'static HashSet<&'static str> {
    static SETS: OnceLock<Vec<HashSet<&'static str>>> = OnceLock::new();
    let sets = SETS.get_or_init(|| {
        [WORDS_EN, WORDS_ES, WORDS_PT, WORDS_IT, WORDS_FR, WORDS_DE].iter().map(|w| w.split_whitespace().collect()).collect()
    });
    &sets[LANGS.iter().position(|l| *l == lang).unwrap()]
}

/// The tokens a line gives the language scorer: each word lower-cased with its edge punctuation off, and, for a
/// word with an apostrophe inside, also the part up to and with the apostrophe ("j'", "l'").
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in line.split_whitespace() {
        let t: String = w.to_lowercase().replace('’', "'");
        let t = t.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'');
        if t.is_empty() {
            continue;
        }
        if let Some(i) = t.find('\'').filter(|i| *i > 0 && *i + 1 < t.len()) {
            out.push(t[..=i].to_string());
        }
        out.push(t.to_string());
    }
    out
}

/// Evidence for each language in a line (×2: a function word is 2, a letter only one language spells 2-6).
fn scores(line: &str) -> [u32; 6] {
    let mut s = [0u32; 6];
    for t in tokens(line) {
        for (k, l) in LANGS.iter().enumerate() {
            s[k] += 2 * u32::from(word_set(*l).contains(t.as_str()));
        }
    }
    for c in line.chars().flat_map(char::to_lowercase) {
        let (es, pt, it, fr, de) = match c {
            'ñ' => (6, 0, 0, 0, 0),
            'ã' | 'õ' => (0, 6, 0, 0, 0),
            'ß' => (0, 0, 0, 0, 6),
            'ä' | 'ö' => (0, 0, 0, 0, 3),
            'ü' => (1, 0, 0, 0, 3),
            'œ' => (0, 0, 0, 6, 0),
            'ç' => (0, 2, 0, 2, 0),
            'ê' | 'â' | 'ô' => (0, 1, 0, 2, 0),
            'î' | 'û' | 'ë' | 'ï' => (0, 0, 0, 3, 0),
            'ì' | 'ò' => (0, 0, 3, 0, 0),
            'è' | 'ù' => (0, 0, 2, 2, 0),
            'à' => (0, 1, 2, 2, 0),
            'á' | 'í' | 'ó' | 'ú' => (2, 2, 0, 0, 0),
            'é' => (1, 1, 1, 1, 0),
            _ => (0, 0, 0, 0, 0),
        };
        s[1] += es;
        s[2] += pt;
        s[3] += it;
        s[4] += fr;
        s[5] += de;
    }
    s
}

/// The best language in a score row: the one with most evidence, English on a tie (and under `least` evidence).
fn best(s: &[u32; 6], least: u32) -> Lang {
    let (mut k, mut m) = (0, s[0]);
    for (i, v) in s.iter().enumerate().skip(1) {
        if *v > m {
            (k, m) = (i, *v);
        }
    }
    if m < least { Lang::En } else { LANGS[k] }
}

/// The language the song's lyrics are mostly in, from their function words and accented letters; English unless
/// another language has clearly more evidence (at least 3 function words' worth).
pub fn detect(lines: &[(f64, String)]) -> Lang {
    let mut total = [0u32; 6];
    for (_, l) in lines {
        for (t, s) in total.iter_mut().zip(scores(l)) {
            *t += s;
        }
    }
    best(&total, 6)
}

/// The language of each line: the song's (`detect`), unless a line on its own clearly belongs to another (an
/// English chorus in a Spanish song, a Spanish verse in an English one): at least 2 function words of it and more
/// than twice the evidence for the song's language. Ties keep the song's language, so vocables and shared words
/// ("no no no") change nothing.
pub fn line_langs(lines: &[(f64, String)]) -> Vec<Lang> {
    let song = detect(lines);
    let si = LANGS.iter().position(|l| *l == song).unwrap();
    lines.iter().map(|(_, l)| {
        let s = scores(l);
        let b = best(&s, 4);
        let bi = LANGS.iter().position(|l| *l == b).unwrap();
        if b != song && s[bi] > 2 * s[si] { b } else { song }
    }).collect()
}

/// How a letter sounds for syllable counting: a vowel that can carry a syllable on its own (strong), one that
/// leans on a neighbouring vowel in a diphthong (weak: i, u), a consonant, or no letter at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind { Strong, Weak, Consonant, Other }

fn is_vowel_letter(c: char, lang: Lang) -> bool {
    "aeiouáàâäãåéèêëíìîïóòôöõúùûüýÿ".contains(c) || (lang == Lang::Fr && "œæ".contains(c)) || (lang == Lang::De && "äöü".contains(c))
}

/// Which letters are vowels and how (per language), with 'y' a vowel unless a vowel follows it ("hoy", "pays",
/// "y" alone; not "yo", "ayer", "yeux") and 'u' a consonant after 'q' and after 'g' before e/i ("quiero",
/// "guerra", "ninguém"), but a vowel elsewhere ("agua", "cuore").
fn kinds(chars: &[char], lang: Lang) -> Vec<Kind> {
    let low: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let n = low.len();
    (0..n).map(|i| {
        let c = low[i];
        if c == 'y' {
            let next_vowel = low.get(i + 1).is_some_and(|d| is_vowel_letter(*d, lang) || *d == 'y');
            return if next_vowel { Kind::Consonant } else { Kind::Weak };
        }
        if c == 'u' {
            let prev = if i > 0 { Some(low[i - 1]) } else { None };
            let next_ei = low.get(i + 1).is_some_and(|d| "eiéíèìêî".contains(*d));
            if prev == Some('q') || (prev == Some('g') && next_ei) {
                return Kind::Consonant;
            }
        }
        if !is_vowel_letter(c, lang) {
            return if c.is_alphabetic() { Kind::Consonant } else { Kind::Other };
        }
        match lang {
            // Spanish and Portuguese: an accented i or u is stressed and makes a hiatus (día, país, saúde): strong
            Lang::Es | Lang::Pt => if "iuüy".contains(c) { Kind::Weak } else { Kind::Strong },
            // Italian: an accent on i/u marks the stress of a final syllable (più, giù, così): still a diphthong
            Lang::It => if "iuìùíúü".contains(c) { Kind::Weak } else { Kind::Strong },
            // French and German group vowels by digraph, not strength (see nuclei)
            _ => Kind::Strong,
        }
    }).collect()
}

/// The vowel nuclei of a word (the syllable count): each as the char indices (first, last) of the letters in it.
fn nuclei(chars: &[char], kind: &[Kind], lang: Lang) -> Vec<(usize, usize)> {
    let low: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if kind[i] == Kind::Consonant || kind[i] == Kind::Other {
            i += 1;
            continue;
        }
        // a run of vowel letters
        let mut j = i;
        while j + 1 < chars.len() && (kind[j + 1] == Kind::Strong || kind[j + 1] == Kind::Weak) {
            j += 1;
        }
        match lang {
            Lang::Fr => {
                // one nucleus per run, except that an accented é/è/ê or a diaeresis (ë, ï, ü) starts its own
                // (poème, créé, réussir, Noël, naïve)
                let mut k = i;
                let mut start = i;
                while k <= j {
                    let c = low[k];
                    let own = "éèêëïü".contains(c);
                    if own && k > start {
                        out.push((start, k - 1));
                        start = k;
                    }
                    if own {
                        out.push((start, k));
                        start = k + 1;
                    }
                    k += 1;
                }
                if start <= j {
                    out.push((start, j));
                }
            }
            Lang::De => {
                // diphthongs and long vowels (ei, ai, au, eu, äu, ie, ey, ay, aa, ee, oo) are one; other
                // neighbours are hiatus (Oase, Ruine, Theater); runs greedily paired from the left (Feier, Frauen)
                let mut k = i;
                while k <= j {
                    let pair = k < j && matches!((low[k], low[k + 1]),
                        ('e', 'i') | ('a', 'i') | ('a', 'u') | ('e', 'u') | ('ä', 'u') | ('i', 'e') | ('e', 'y') | ('a', 'y') | ('a', 'a') | ('e', 'e') | ('o', 'o') | ('o', 'i'));
                    if pair {
                        out.push((k, k + 1));
                        k += 2;
                    } else {
                        out.push((k, k));
                        k += 1;
                    }
                }
            }
            Lang::Es | Lang::Pt | Lang::It => {
                // a nucleus is [weak] strong [weak] (rising, falling, or both: tiempo, aire, buey), or two weak
                // vowels (ruido, viu); two strong vowels are two syllables (poeta, leer); a weak vowel between two
                // strong ones rises with the second (gio-ia, i-de-ia, pra-ia); Portuguese's nasal ão, ãe, õe are one
                let mut k = i;
                while k <= j {
                    let mut end = k;
                    if kind[k] == Kind::Weak {
                        if end < j && kind[end + 1] == Kind::Strong {
                            end += 1;
                            if end < j && kind[end + 1] == Kind::Weak && !(end + 1 < j && kind[end + 2] == Kind::Strong) {
                                end += 1;
                            }
                        } else if end < j && kind[end + 1] == Kind::Weak {
                            end += 1;
                        }
                    } else if end < j && kind[end + 1] == Kind::Weak && !(end + 1 < j && kind[end + 2] == Kind::Strong) {
                        end += 1;
                    } else if lang == Lang::Pt && end < j && matches!((low[end], low[end + 1]), ('ã', 'o') | ('ã', 'e') | ('õ', 'e')) {
                        end += 1;        // nasal diphthongs: não, mãe, põe
                    }
                    out.push((k, end));
                    k = end + 1;
                }
            }
            Lang::En => out.push((i, j)),
        }
        i = j + 1;
    }
    hiatus_exceptions(&low, kind, lang, out)
}

/// Rising diphthongs that are really two syllables: Italian's stressed i/u before a vowel in a few common words
/// (io, mio, tuo, via, zio, due, sia), and Portuguese's word-final -ia, -io, -ua, -uo, -ie, -ue after a consonant in a
/// word with no other accent (dia, lua, sua, queria, alegria; not glória, água, família, where the accent says the
/// stress is elsewhere, nor ideia, where the i falls with the e).
fn hiatus_exceptions(low: &[char], kind: &[Kind], lang: Lang, mut nuclei: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let letters: String = low.iter().filter(|c| c.is_alphabetic()).collect();
    let split_last_rising = |nuclei: &mut Vec<(usize, usize)>| {
        if let Some(&(a, b)) = nuclei.last() {
            if b == a + 1 && kind[a] == Kind::Weak && kind[b] == Kind::Strong {
                nuclei.pop();
                nuclei.push((a, a));
                nuclei.push((b, b));
            }
        }
    };
    match lang {
        Lang::It if matches!(letters.as_str(), "io" | "mio" | "mia" | "mie" | "tuo" | "tua" | "tue" | "suo" | "sua" | "sue" | "dio" | "via" | "vie" | "zio" | "zia" | "zie" | "due" | "bue" | "sia" | "pio" | "pia") => {
            split_last_rising(&mut nuclei);
        }
        Lang::Pt => {
            let stem = letters.strip_suffix('s').unwrap_or(&letters);
            let accented = letters.chars().any(|c| "áâéêíóôúàã".contains(c));
            let final_rising = stem.len() >= 3 && ["ia", "io", "ua", "uo", "ie", "ue"].iter().any(|e| stem.ends_with(e))
                && stem.chars().rev().nth(2).is_some_and(|c| !is_vowel_letter(c, lang));
            if final_rising && !accented {
                // (the last nucleus may carry a trailing 's' in the chars but not in the nucleus itself)
                split_last_rising(&mut nuclei);
            }
        }
        _ => {}
    }
    nuclei
}

/// Consonant clusters that begin a syllable whole (never split inside): digraphs and stop/fricative + l/r, per
/// language, plus Italian's s + consonant (que-sto, fe-sta) and German's sch- clusters. Anything longer loses its
/// first letters to the syllable before (es-tre-lla, Fens-ter, sem-pre).
fn onsets(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Es => &["bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "tr", "ch", "ll", "rr", "qu", "gu"],
        Lang::Pt => &["bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "tr", "vr", "ch", "lh", "nh", "qu", "gu"],
        Lang::It => &["bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "tr", "vr", "ch", "gh", "gn", "sc", "qu",
                      "sb", "sd", "sf", "sg", "sl", "sm", "sn", "sp", "sq", "st", "sv", "str", "spr", "scr", "sbr", "sdr", "sfr", "sgr", "spl", "scl", "sch"],
        Lang::Fr => &["bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "tr", "vr", "ch", "ph", "th", "gn", "qu", "gu"],
        Lang::De => &["bl", "br", "dr", "fl", "fr", "gl", "gr", "kl", "kr", "pl", "pr", "tr", "zw", "ch", "ck", "sch", "ph", "th", "pf", "qu",
                      "schl", "schr", "schw", "schm", "schn"],
        Lang::En => &[],
    }
}

/// A word split into syllables by `lang`'s spelling (not English: see syllables::Syllabifier::split for that).
/// The parts join back to the word exactly (punctuation and case kept), so the bars' labels can be found in the
/// lyrics and their letters counted against the aligner's.
///  - count: the vowel nuclei (`nuclei`), less a French word-final silent e/es after a consonant when the word
///    has another syllable (chante, étoile, table; not je, vie, amie, été); -ent is kept (souvent, moment,
///    vraiment are sounded; the verb ending isn't, and is over-counted)
///  - cut: before the onset the consonants between two nuclei can begin (`onsets`), else before the last of
///    them; nothing between two nuclei: between the vowels (po-e-ta, Ru-i-ne)
pub fn syllables(word: &str, lang: Lang) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let kind = kinds(&chars, lang);
    let mut nuc = nuclei(&chars, &kind, lang);
    if lang == Lang::Fr && nuc.len() >= 2 {
        let (a, b) = nuc[nuc.len() - 1];
        let low: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
        let tail: String = low[b + 1..].iter().filter(|c| c.is_alphabetic()).collect();
        if a == b && low[a] == 'e' && (tail.is_empty() || tail == "s") && a > 0 && kind[a - 1] == Kind::Consonant {
            nuc.pop();
        }
    }
    if nuc.len() < 2 {
        return vec![word.to_string()];
    }
    let low: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut cuts = Vec::new();
    for w in nuc.windows(2) {
        let (end_a, start_b) = (w[0].1, w[1].0);
        let cons: Vec<usize> = (end_a + 1..start_b).filter(|&k| kind[k] == Kind::Consonant).collect();
        let cut = if cons.is_empty() {
            start_b
        } else {
            let letters: Vec<char> = cons.iter().map(|&k| low[k]).collect();
            let onset_len = (1..=cons.len().min(4)).rev()
                .find(|&n| n == 1 || onsets(lang).contains(&letters[letters.len() - n..].iter().collect::<String>().as_str()))
                .unwrap_or(1);
            cons[cons.len() - onset_len]
        };
        cuts.push(cut);
    }
    let mut parts = Vec::with_capacity(cuts.len() + 1);
    let mut last = 0;
    for c in cuts {
        parts.push(chars[last..c].iter().collect::<String>());
        last = c;
    }
    parts.push(chars[last..].iter().collect());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<(f64, String)> {
        v.iter().enumerate().map(|(i, s)| (i as f64 * 3.0, s.to_string())).collect()
    }

    fn count(w: &str, lang: Lang) -> usize {
        syllables(w, lang).len()
    }

    fn check(lang: Lang, cases: &[(&str, &str)]) {
        let mut wrong = Vec::new();
        for (w, want) in cases {
            let got = syllables(w, lang).join("-");
            if got != *want {
                wrong.push(format!("{w}: {got} (want {want})"));
            }
        }
        assert!(wrong.is_empty(), "{:?}: {}", lang, wrong.join("; "));
    }

    #[test]
    fn scripts_of_letters() {
        assert_eq!(script_of('a'), Some(Script::Latin));
        assert_eq!(script_of('ñ'), Some(Script::Latin));
        assert_eq!(script_of('ẫ'), Some(Script::Latin));
        assert_eq!(script_of('日'), Some(Script::Han));
        assert_eq!(script_of('か'), Some(Script::Kana));
        assert_eq!(script_of('ア'), Some(Script::Kana));
        assert_eq!(script_of('한'), Some(Script::Hangul));
        assert_eq!(script_of('क'), Some(Script::Devanagari));
        assert_eq!(script_of('ก'), Some(Script::Thai));
        assert_eq!(script_of('д'), Some(Script::Cyrillic));
        assert_eq!(script_of('ع'), Some(Script::Arabic));
        assert_eq!(script_of('ש'), Some(Script::Hebrew));
        assert_eq!(script_of('λ'), Some(Script::Greek));
        assert_eq!(script_of('1'), None);
        assert_eq!(script_of('、'), None);
        assert_eq!(script_of(' '), None);
        assert_eq!(word_script("日本"), Some(Script::Han));
        assert_eq!(word_script("corazón,"), Some(Script::Latin));
        assert_eq!(word_script("..."), None);
    }

    #[test]
    fn a_song_in_another_script_is_not_alignable_but_a_mixed_one_is() {
        let han = lines(&["春天的花开了", "我们一起唱歌", "月亮照在河上", "风吹过树林的声音"]);
        assert_eq!(song_script(&han), Script::Han);
        assert!(!alignable(&han));
        // one English word in a Japanese song changes nothing
        let mut kana = lines(&["ひかりのなかで", "うたをうたおう", "よるのそらにほしが", "かぜがふいている"]);
        kana.push((12.0, "Oh yeah".into()));
        assert_eq!(song_script(&kana), Script::Kana);
        assert!(!alignable(&kana));
        let hangul = lines(&["노래를 불러요", "하늘이 파래요", "바람이 불어요", "우리는 함께 가요"]);
        assert_eq!(song_script(&hangul), Script::Hangul);
        assert!(!alignable(&hangul));
        let hindi = lines(&["गीत गाओ मेरे साथ", "आसमान नीला है", "हवा चलती है", "हम साथ चलते हैं"]);
        assert_eq!(song_script(&hindi), Script::Devanagari);
        assert!(!alignable(&hindi));
        // a Korean song with English hook lines: the English words can be aligned
        let mixed = lines(&["노래를 불러요", "Baby you know I want you tonight", "하늘이 파래요", "Baby you know I need you tonight",
                            "바람이 불어요", "Dance with me all night long"]);
        assert_eq!(song_script(&mixed), Script::Latin);
        assert!(alignable(&mixed));
        let en = lines(&["hello there my friend", "the sun is shining bright", "we sing a song", "all night long"]);
        assert!(alignable(&en));
        assert!(!alignable(&lines(&["la", "oh"])));
        assert!(!alignable(&[]));
    }

    #[test]
    fn detects_latin_languages_from_function_words_and_accents() {
        assert_eq!(detect(&lines(&["I know that you love me tonight", "and the night is young", "we can dance all night long"])), Lang::En);
        assert_eq!(detect(&lines(&["yo no sé por qué te quiero tanto", "mi corazón canta para ti", "y la noche es nuestra"])), Lang::Es);
        assert_eq!(detect(&lines(&["eu não sei por que você canta", "meu coração é teu", "e a noite é nossa"])), Lang::Pt);
        assert_eq!(detect(&lines(&["non so perché ti amo così", "il mio cuore canta per te", "e la notte è nostra"])), Lang::It);
        assert_eq!(detect(&lines(&["je ne sais pas pourquoi je t'aime", "mon cœur chante pour toi", "et la nuit est à nous"])), Lang::Fr);
        assert_eq!(detect(&lines(&["ich weiß nicht warum ich dich liebe", "mein Herz singt für dich", "und die Nacht ist unser"])), Lang::De);
        // vocables and a stray shared word say nothing: English
        assert_eq!(detect(&lines(&["la la la la", "na na na", "oh oh"])), Lang::En);
        assert_eq!(detect(&[]), Lang::En);
        // an English song with one Spanish word stays English
        assert_eq!(detect(&lines(&["I love you mi amor", "you are the one for me", "and I will never let you go"])), Lang::En);
    }

    #[test]
    fn mixed_songs_get_a_language_per_line() {
        let es = lines(&["yo no sé por qué te quiero tanto", "Baby you know I want you tonight", "mi corazón canta para ti",
                         "la la la la", "no no no no"]);
        assert_eq!(line_langs(&es), vec![Lang::Es, Lang::En, Lang::Es, Lang::Es, Lang::Es]);
        let en = lines(&["I know that you love me tonight", "and the night is young", "yo no sé por qué te quiero tanto",
                         "oh la la", "say no more, no"]);
        assert_eq!(line_langs(&en), vec![Lang::En, Lang::En, Lang::Es, Lang::En, Lang::En]);
    }

    #[test]
    fn spanish_syllables() {
        check(Lang::Es, &[
            ("come", "co-me"), ("dame", "da-me"), ("vive", "vi-ve"), ("sale", "sa-le"), ("corazón", "co-ra-zón"), ("canción", "can-ción"),
            ("quiero", "quie-ro"), ("tiempo", "tiem-po"), ("siempre", "siem-pre"), ("hombre", "hom-bre"), ("estrella", "es-tre-lla"),
            ("calle", "ca-lle"), ("perro", "pe-rro"), ("mucho", "mu-cho"), ("niño", "ni-ño"), ("agua", "a-gua"), ("guitarra", "gui-ta-rra"),
            ("ahora", "a-ho-ra"), ("poeta", "po-e-ta"), ("leer", "le-er"), ("día", "dí-a"), ("país", "pa-ís"), ("río", "rí-o"),
            ("después", "des-pués"), ("adiós", "a-diós"), ("hoy", "hoy"), ("rey", "rey"), ("yo", "yo"), ("ayer", "a-yer"), ("y", "y"),
            ("buey", "buey"), ("ruido", "rui-do"), ("cielo", "cie-lo"), ("nunca", "nun-ca"), ("bailar", "bai-lar"), ("aire", "ai-re"),
            ("Corazón,", "Co-ra-zón,"), ("¡vive!", "¡vi-ve!"), ("sé", "sé"), ("más", "más"), ("alegría", "a-le-grí-a"), ("también", "tam-bién"),
        ]);
    }

    #[test]
    fn portuguese_syllables() {
        check(Lang::Pt, &[
            ("coração", "co-ra-ção"), ("você", "vo-cê"), ("saudade", "sau-da-de"), ("não", "não"), ("mãe", "mãe"), ("dia", "di-a"),
            ("lua", "lu-a"), ("sua", "su-a"), ("glória", "gló-ria"), ("água", "á-gua"), ("família", "fa-mí-lia"), ("queria", "que-ri-a"),
            ("alegria", "a-le-gri-a"), ("ideia", "i-de-ia"), ("meu", "meu"), ("céu", "céu"), ("noite", "noi-te"), ("ninguém", "nin-guém"),
            ("chuva", "chu-va"), ("olhos", "o-lhos"), ("sonho", "so-nho"), ("filho", "fi-lho"), ("sempre", "sem-pre"), ("estrela", "es-tre-la"),
            ("paixão", "pai-xão"), ("beijo", "bei-jo"), ("quando", "quan-do"), ("poesia", "po-e-si-a"), ("praia", "pra-ia"), ("tudo", "tu-do"),
            ("amor", "a-mor"), ("pra", "pra"), ("canção", "can-ção"),
        ]);
    }

    #[test]
    fn italian_syllables() {
        check(Lang::It, &[
            ("amore", "a-mo-re"), ("cuore", "cuo-re"), ("notte", "not-te"), ("bello", "bel-lo"), ("stella", "stel-la"), ("figlio", "fi-glio"),
            ("bagno", "ba-gno"), ("pesce", "pe-sce"), ("tutto", "tut-to"), ("sempre", "sem-pre"), ("questo", "que-sto"), ("cielo", "cie-lo"),
            ("più", "più"), ("perché", "per-ché"), ("città", "cit-tà"), ("io", "i-o"), ("mio", "mi-o"), ("mai", "mai"), ("noi", "noi"),
            ("vuoi", "vuoi"), ("miei", "miei"), ("piano", "pia-no"), ("fiore", "fio-re"), ("gioia", "gio-ia"), ("due", "du-e"), ("via", "vi-a"),
            ("sei", "sei"), ("maschera", "ma-sche-ra"), ("festa", "fe-sta"), ("insieme", "in-sie-me"), ("vita", "vi-ta"), ("così", "co-sì"),
        ]);
    }

    #[test]
    fn french_syllables() {
        check(Lang::Fr, &[
            ("chanson", "chan-son"), ("étoile", "é-toile"), ("amour", "a-mour"), ("je", "je"), ("vie", "vie"), ("amie", "a-mie"),
            ("chante", "chante"), ("chantes", "chantes"), ("table", "table"), ("elle", "elle"), ("les", "les"), ("été", "é-té"), ("créé", "cré-é"),
            ("poème", "po-ème"), ("Noël", "No-ël"), ("naïve", "na-ïve"), ("oui", "oui"), ("nuit", "nuit"), ("pluie", "pluie"), ("beau", "beau"),
            ("nation", "na-tion"), ("toujours", "tou-jours"), ("souvent", "sou-vent"), ("moment", "mo-ment"), ("yeux", "yeux"), ("pays", "pays"),
            ("payer", "pa-yer"), ("qui", "qui"), ("guerre", "guerre"), ("cœur", "cœur"), ("rêve", "rêve"), ("ensemble", "en-semble"), ("c'est", "c'est"),
            ("l'amour", "l'a-mour"), ("j'ai", "j'ai"), ("dieu", "dieu"), ("réussir", "ré-us-sir"), ("liberté", "li-ber-té"),
        ]);
        // the silent final e stays in the last part (no bar of its own), so the parts still join to the word
        assert_eq!(syllables("ensemble,", Lang::Fr), vec!["en", "semble,"]);
        assert_eq!(count("chantes", Lang::Fr), 1);
        assert_eq!(count("étoile", Lang::Fr), 2);
    }

    #[test]
    fn german_syllables() {
        check(Lang::De, &[
            ("Liebe", "Lie-be"), ("Straße", "Stra-ße"), ("Leben", "Le-ben"), ("Freundin", "Freun-din"), ("Träume", "Träu-me"), ("Schnee", "Schnee"),
            ("Boot", "Boot"), ("Meer", "Meer"), ("die", "die"), ("nie", "nie"), ("wie", "wie"), ("ruhig", "ru-hig"), ("Ruine", "Ru-i-ne"),
            ("Theater", "The-a-ter"), ("Europa", "Eu-ro-pa"), ("Poesie", "Po-e-sie"), ("neu", "neu"), ("Mäuse", "Mäu-se"), ("Sonne", "Son-ne"),
            ("Wasser", "Was-ser"), ("Himmel", "Him-mel"), ("Mutter", "Mut-ter"), ("Herzen", "Her-zen"), ("Mädchen", "Mäd-chen"), ("zwischen", "zwi-schen"),
            ("lächeln", "lä-cheln"), ("Fenster", "Fens-ter"), ("Zucker", "Zu-cker"), ("Finger", "Fin-ger"), ("Katze", "Kat-ze"), ("Feier", "Fei-er"),
            ("Frauen", "Frau-en"), ("neue", "neu-e"), ("Idee", "I-dee"), ("Welt", "Welt"), ("nicht", "nicht"), ("gestern", "ges-tern"), ("Oase", "O-a-se"),
            ("Bayern", "Ba-yern"), ("Nacht", "Nacht"), ("Herz", "Herz"), ("alles", "al-les"), ("immer", "im-mer"), ("zusammen", "zu-sam-men"),
            ("größte", "größ-te"), ("außen", "au-ßen"), ("Füße", "Fü-ße"), ("Grüße", "Grü-ße"), ("heißt", "heißt"), ("Schloß", "Schloß"),
        ]);
    }

    #[test]
    fn parts_always_join_back_to_the_word_and_a_word_without_vowels_is_one() {
        for lang in [Lang::Es, Lang::Pt, Lang::It, Lang::Fr, Lang::De] {
            for w in ["corazón,", "¿qué?", "l'amour", "Straße!", "pst", "hmm", "123", "日本", "", "—", "O'Brien", "x"] {
                let p = syllables(w, lang);
                assert_eq!(p.join(""), w, "{lang:?} {w}");
                assert!(!p.is_empty());
            }
            assert_eq!(count("pst", lang), 1);
            assert_eq!(count("日本", lang), 1);
        }
    }

    #[test]
    fn counts_for_invented_phrases() {
        let total = |s: &str, lang: Lang| s.split_whitespace().map(|w| count(w, lang)).sum::<usize>();
        assert_eq!(total("dame tu corazón ahora", Lang::Es), 2 + 1 + 3 + 3);
        assert_eq!(total("você canta com saudade", Lang::Pt), 2 + 2 + 1 + 3);
        assert_eq!(total("il cuore canta sempre", Lang::It), 1 + 2 + 2 + 2);
        assert_eq!(total("une chanson pour toi", Lang::Fr), 1 + 2 + 1 + 1);
        assert_eq!(total("meine Liebe bleibt immer", Lang::De), 2 + 2 + 1 + 2);
    }
}
