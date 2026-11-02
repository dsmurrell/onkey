import { codeUrl } from "@/lib/site";
import { getT } from "@/lib/i18n";

/** What OnKey is built on: the name and link as the project writes them, what it's used for (a key in
 *  docs.credits.uses), and its licence (as named, or a key in docs.credits.licences for the ones that aren't
 *  licences: a service, part of macOS). */
type Row = [name: string, url: string, use: Use, licence: string | { key: "service" | "macos" }];
type Use = "crepe" | "game" | "wav2vec" | "uvr" | "cmudict" | "lrclib" | "shazamkit" | "itunes" | "hyphenation" | "onnx" | "tauri";
const ROWS: Row[] = [
  ["CREPE (via torchcrepe)", "https://github.com/marl/crepe", "crepe", "MIT"],
  ["GAME (OpenVPI)", "https://github.com/openvpi/GAME", "game", "MIT"],
  ["wav2vec 2.0 base 960h (torchaudio / fairseq)", "https://github.com/facebookresearch/fairseq", "wav2vec", "MIT"],
  ["UVR MDX-Net Karaoke 2 (Ultimate Vocal Remover)", "https://github.com/Anjok07/ultimatevocalremovergui", "uvr", "MIT terms, with credit to UVR (Anjok07, aufr33)"],
  ["CMU Pronouncing Dictionary", "http://www.speech.cs.cmu.edu/cgi-bin/cmudict", "cmudict", "BSD-style"],
  ["LRCLIB", "https://lrclib.net", "lrclib", { key: "service" }],
  ["ShazamKit (Apple)", "https://developer.apple.com/shazamkit/", "shazamkit", { key: "macos" }],
  ["iTunes Search API (Apple)", "https://performance-partners.apple.com/search-api", "itunes", { key: "service" }],
  ["US English hyphenation patterns (hyph-utf8)", "https://github.com/hyphenation/tex-hyphen", "hyphenation", "Permissive (Kuiken)"],
  ["ONNX Runtime", "https://onnxruntime.ai", "onnx", "MIT"],
  ["Tauri", "https://tauri.app", "tauri", "MIT / Apache-2.0"],
];

export function creditsTitle(locale: string) {
  return getT(locale, "docs.credits").t("title");
}

export function CreditsPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.credits");
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">{t("lede")}</p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("component")}</th><th>{t("usedFor")}</th><th>{t("licence")}</th></tr></thead>
          <tbody>
            {ROWS.map(([name, url, use, lic]) => (
              <tr key={name}><td><a href={url}>{name}</a></td><td>{t(`uses.${use}`)}</td><td>{typeof lic === "string" ? lic : t(`licences.${lic.key}`)}</td></tr>
            ))}
          </tbody>
        </table>
      </div>
      <p>{rich("notices", { link: () => <a href={codeUrl("THIRD_PARTY_NOTICES.md")}>THIRD_PARTY_NOTICES.md</a> })}</p>
    </>
  );
}
