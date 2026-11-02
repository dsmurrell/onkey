import { SiteBody, VIEWPORT, htmlClass, siteMetadata } from "@/components/SiteBody";
import "../globals.css";

// English, at the site's root: /, /docs/…, /changelog/ (the URLs the app and release notes link to).
export const metadata = siteMetadata("en");
export const viewport = VIEWPORT;

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={htmlClass}>
      <SiteBody locale="en">{children}</SiteBody>
    </html>
  );
}
