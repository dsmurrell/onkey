import type { Metadata } from "next";
import { SiteBody, VIEWPORT, htmlClass, siteMetadata } from "@/components/SiteBody";
import { otherLocales } from "@/lib/i18n";
import "../globals.css";

// Every other language: /es/, /es/docs/faq/, … one per catalog in site/messages/. These files end in .i18n.tsx
// so that next.config.ts can leave the route out while English is the only catalog (a static export refuses
// a dynamic route with no pages).
export const dynamicParams = false;
export const generateStaticParams = () => otherLocales().map((locale) => ({ locale }));
export const viewport = VIEWPORT;

type Props = { children: React.ReactNode; params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Omit<Props, "children">): Promise<Metadata> {
  return siteMetadata((await params).locale);
}

export default async function LocaleLayout({ children, params }: Props) {
  const { locale } = await params;
  return (
    <html lang={locale} className={htmlClass}>
      <SiteBody locale={locale}>{children}</SiteBody>
    </html>
  );
}
