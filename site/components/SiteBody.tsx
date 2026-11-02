import type { Metadata, Viewport } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Script from "next/script";
import { Header } from "@/components/Header";
import { Footer } from "@/components/Footer";
import { FlowMyna } from "@/components/FlowMyna";
import { PLAUSIBLE_DOMAIN, SHARE_IMAGE, SITE_URL } from "@/lib/site";
import { DEFAULT_LOCALE } from "@/lib/languages";
import { getT } from "@/lib/i18n";

// What every page has, in every language: the two root layouts (app/(en)/ for English at /, app/[locale]/ for
// /es/ and the others) differ only in <html lang>.

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({ variable: "--font-geist-mono", subsets: ["latin"] });
export const htmlClass = `${geistSans.variable} ${geistMono.variable}`;

export function siteMetadata(locale: string): Metadata {
  const { t } = getT(locale, "meta");
  const description = t("description");
  const home = locale === DEFAULT_LOCALE ? "/" : `/${locale}/`;
  return {
    title: { default: t("title"), template: "%s · OnKey" },
    description,
    // Link previews need the site's own address (LAUNCH.siteUrl in lib/site.ts).
    ...(SITE_URL && {
      metadataBase: new URL(SITE_URL),
      openGraph: { title: "OnKey", description, url: home, siteName: "OnKey", type: "website", images: [{ url: SHARE_IMAGE, width: 1280, height: 640, alt: t("title") }] },
      twitter: { card: "summary_large_image", title: "OnKey", description, images: [SHARE_IMAGE] },
    }),
  };
}

export const VIEWPORT: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: dark)", color: "#07080c" },
    { media: "(prefers-color-scheme: light)", color: "#f6f6fb" },
  ],
};

export function SiteBody({ locale, children }: { locale: string; children: React.ReactNode }) {
  const { t } = getT(locale, "common");
  return (
    <body className="flex min-h-dvh flex-col">
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-3 focus:z-50 focus:rounded-lg focus:bg-panel focus:px-3 focus:py-2">
        {t("skip")}
      </a>
      <Header locale={locale} />
      <main id="main" className="flex-1">{children}</main>
      <Footer locale={locale} />
      {/* visit counts without cookies or personal data (Plausible); outbound links (download, GitHub, Discord)
          are counted as they're followed. Plausible ignores localhost. */}
      <FlowMyna />
      {PLAUSIBLE_DOMAIN && (
        <Script defer data-domain={PLAUSIBLE_DOMAIN} src="https://plausible.io/js/script.outbound-links.js" strategy="afterInteractive" />
      )}
    </body>
  );
}
