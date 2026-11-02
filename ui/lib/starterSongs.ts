// Songs to try, per language (StarterSongs.tsx, Setup's last step): picked to be easy to follow (a clean lead vocal,
// a steady melody, well known), easy to hard. Titles and artists only, plus where to find each: no lyrics, audio or
// covers bundled. `why` is its message under starters.why.<list>.<why> (English in messages/en.json; translators
// say it in their language). `spotify`: the track's id (open.spotify.com/track/<id>); `lrclib`: LRCLIB's id for
// its synced lyrics (duration within 1 s). The lists other than English: approved by Daniel, 1 Nov 2026.
//
// Which list: the window's language if it's Full and has one (es-ES uses es's), else English. A language may also
// have a starter playlist; none yet but English's.
import { languageOf } from "./languages";

export type StarterSong = { artist: string; title: string; why: string; spotify?: string; lrclib?: number };
export type StarterList = { songs: StarterSong[];
  /** the OnKey starter playlist, public: a Spotify one (https://open.spotify.com/playlist/<id>) and/or an Apple
   *  Music one (https://music.apple.com/...). Neither: no playlist card. */
  playlist?: string; playlistApple?: string };

export const STARTERS: Record<string, StarterList> = {
  en: {
    playlist: "https://open.spotify.com/playlist/3Anv0qIDPKoitFPK71u7Bd",
    songs: [
      { artist: "Ben E. King", title: "Stand By Me", why: "standByMe", spotify: "3SdTKo2uVsxFblQjpScoHy" },
      { artist: "Oasis", title: "Wonderwall", why: "wonderwall", spotify: "1qPbGZqppFwLwcBC1JQ6Vr" },
      { artist: "Tracy Chapman", title: "Fast Car", why: "fastCar", spotify: "2M9ro2krNb7nr7HSprkEgo" },
      { artist: "Lana Del Rey", title: "Young And Beautiful", why: "youngAndBeautiful", spotify: "2nMeu6UenVvwUktBCpLMK9" },
      { artist: "Lewis Capaldi", title: "Someone You Loved", why: "someoneYouLoved", spotify: "7qEHsqek33rTcFNT9PFqLf" },
      { artist: "Adele", title: "Someone Like You", why: "someoneLikeYou", spotify: "3bNv3VuUOKgrf5hu3YcuRo" },
      { artist: "Jeff Buckley", title: "Hallelujah", why: "hallelujah", spotify: "3Vby3j8R69ZFAGNxIfobKj" },
      { artist: "Whitney Houston", title: "I Will Always Love You", why: "iWillAlwaysLoveYou", spotify: "4eHbdreAnSOrDDsFfc4Fpm" },
    ],
  },
  es: {
    songs: [
      { artist: "Juanes", title: "La Camisa Negra", why: "laCamisaNegra", spotify: "5jW8veZpHmgrGz6qcimuRu", lrclib: 36155127 },
      { artist: "Julieta Venegas", title: "Me Voy", why: "meVoy", spotify: "100Tm6z29RfHTdUQ6NIs0s", lrclib: 34908028 },
      { artist: "Soda Stereo", title: "De Música Ligera", why: "deMusicaLigera", spotify: "4mm3QZKhMvuByfbDYAya2T", lrclib: 35007404 },
      { artist: "Selena", title: "Como La Flor", why: "comoLaFlor", spotify: "0gatssZ0J7pMAlvFCGZZGH", lrclib: 35298865 },
      { artist: "Maná", title: "Rayando el Sol", why: "rayandoElSol", spotify: "4Ofg5uuH7qqDIXpAJMpXZV", lrclib: 36941136 },
      { artist: "La Oreja de Van Gogh", title: "Rosas", why: "rosas", spotify: "4waqcUQWdj0yH26STWl2Rq", lrclib: 16865687 },
      { artist: "Mecano", title: "Hijo de la Luna", why: "hijoDeLaLuna", spotify: "7g3g2sqevicvurLTZ75JTA", lrclib: 33980935 },
      { artist: "Vicente Fernández", title: "Volver, Volver", why: "volverVolver", spotify: "4Fvnz1ZJ86IdqDAepWYPAh", lrclib: 35743790 },
    ],
  },
  pt: {
    songs: [
      { artist: "Lulu Santos", title: "Como uma Onda", why: "comoUmaOnda", spotify: "3cFf4K6ew15k8bYe16kJ9N", lrclib: 38815299 },
      { artist: "Tribalistas", title: "Velha Infância", why: "velhaInfancia", spotify: "1mSxbLW7fKABfeY4lGpg0E", lrclib: 36243289 },
      { artist: "Tim Maia", title: "Gostava Tanto de Você", why: "gostavaTantoDeVoce", spotify: "6eDMElxeeKXs4DHTb0dGP0", lrclib: 36714497 },
      { artist: "Legião Urbana", title: "Tempo Perdido", why: "tempoPerdido", spotify: "7MnT7msJZg3XBAS0OTfGrB", lrclib: 36344709 },
      { artist: "Kid Abelha", title: "Como Eu Quero", why: "comoEuQuero", spotify: "4eq0yOVo6qXCzeVpXKSPoq", lrclib: 36587089 }, // TODO check: the Spotify link may be a remaster or compilation copy
      { artist: "Marília Mendonça", title: "Infiel", why: "infiel", spotify: "38MpAndIjGAyVdcC6npX95", lrclib: 33883080 },
      { artist: "Salvador Sobral", title: "Amar Pelos Dois", why: "amarPelosDois", spotify: "1bLjhPvRT5uNnVby1ytmXi", lrclib: 1654065 },
      { artist: "Djavan", title: "Oceano", why: "oceano", spotify: "3iof6EclqePezHDibBiykI", lrclib: 7812620 },
    ],
  },
  it: {
    songs: [
      { artist: "Adriano Celentano", title: "Azzurro", why: "azzurro", spotify: "0rcDm9dwoLMzkGTul3Z01r", lrclib: 12897601 },
      { artist: "Toto Cutugno", title: "L'italiano", why: "lItaliano", spotify: "2S7RApTsKT0CtYojYq2cKz", lrclib: 36371840 },
      { artist: "Ricchi e Poveri", title: "Sarà perché ti amo", why: "saraPercheTiAmo", spotify: "6lK2xptPzLPmvpE29U4mDH", lrclib: 36183395 },
      { artist: "Vasco Rossi", title: "Albachiara", why: "albachiara", spotify: "4P5Z3iEngFfaVe0qkv4Pdl", lrclib: 34568121 },
      { artist: "Laura Pausini", title: "La solitudine", why: "laSolitudine", spotify: "4frv4JLxEbUmbMukRbcuN8", lrclib: 33981024 },
      { artist: "Lucio Battisti", title: "Emozioni", why: "emozioni", spotify: "3B7plHFM2LK5BVYnwzkD3x", lrclib: 37318225 },
      { artist: "Mina", title: "Grande grande grande", why: "grandeGrandeGrande", spotify: "0T6brAfFrNvhL86aPjQUwN", lrclib: 36182222 }, // TODO check: the Spotify link may be a remaster or compilation copy
      { artist: "Lucio Dalla", title: "Caruso", why: "caruso", spotify: "6HYm04mKq02OtLMf6sGEFR", lrclib: 35143601 },
    ],
  },
  fr: {
    songs: [
      { artist: "Joe Dassin", title: "Les Champs-Élysées", why: "lesChampsElysees", spotify: "4VWbPQUPvLes814r6T11Jz", lrclib: 34003334 },
      { artist: "Francis Cabrel", title: "Je l'aime à mourir", why: "jeLAimeAMourir", spotify: "0camyXKD4coXPQJZ3kch1j", lrclib: 35275956 },
      { artist: "Édith Piaf", title: "La vie en rose", why: "laVieEnRose", spotify: "4FmiciU3ZmfgABlbCSXcWw", lrclib: 36287457 }, // TODO check: the Spotify link may be a remaster or compilation copy
      { artist: "Louane", title: "Avenir", why: "avenir", spotify: "4zDdcSeXrp3jjZpx9EJbAY", lrclib: 14309959 },
      { artist: "Zaz", title: "Je veux", why: "jeVeux", spotify: "734dz1YaFITwawPpM25fSt", lrclib: 13865993 },
      { artist: "Indila", title: "Dernière danse", why: "derniereDanse", spotify: "65uoaqX5qcjXZRheAj1qQT", lrclib: 13865691 },
      { artist: "Charles Aznavour", title: "La Bohème", why: "laBoheme", spotify: "1WvvmEowf7hiz5EnyAwtTj", lrclib: 36226975 },
      { artist: "Céline Dion", title: "Pour que tu m'aimes encore", why: "pourQueTuMAimesEncore", spotify: "6mYSphiU3Qg07g37bZlE4c", lrclib: 36742908 },
    ],
  },
  de: {
    songs: [
      { artist: "Reinhard Mey", title: "Über den Wolken", why: "uberDenWolken", spotify: "0IG6goJ5aYDZDqahHQXjyf", lrclib: 2940230 },
      { artist: "Nena", title: "99 Luftballons", why: "n99Luftballons", spotify: "4ZhPLoMzZwewHLLjV1J15c", lrclib: 12897360 },
      { artist: "Juli", title: "Perfekte Welle", why: "perfekteWelle", spotify: "2LV5joNDrsyuXEh4FBARVK", lrclib: 36392082 },
      { artist: "Helene Fischer", title: "Atemlos durch die Nacht", why: "atemlosDurchDieNacht", spotify: "1cSXzDZt8vzuUp2XREQEJN", lrclib: 36174534 },
      { artist: "Udo Jürgens", title: "Griechischer Wein", why: "griechischerWein", spotify: "6acOaeELSjmb29hNussIXt", lrclib: 34027115 },
      { artist: "Silbermond", title: "Das Beste", why: "dasBeste", spotify: "6KNXoPzCvfsLIjkYV0QRK3", lrclib: 14959778 },
      { artist: "Andreas Bourani", title: "Auf uns", why: "aufUns", spotify: "7CVfM0j9ke7DiTLNULbA9T", lrclib: 33795839 },
      { artist: "Herbert Grönemeyer", title: "Der Weg", why: "derWeg", spotify: "7py0pONdy9D0DxNnT3elik", lrclib: 16116614 },
    ],
  },
};

/** The list for the window's language, and which list it is (its catalog keys: starters.why.<list>.…). */
export function startersFor(locale: string): StarterList & { list: string } {
  const lang = languageOf(locale);
  if (lang?.level === "full") {
    for (const code of [locale, locale.split("-")[0]]) if (STARTERS[code]) return { ...STARTERS[code], list: code };
  }
  return { ...STARTERS.en, list: "en" };
}
