import type { Metadata } from "next";
// Next's own 404 message, as the site showed it inside its layout before it had a layout per language
import NotFound from "next/dist/client/components/builtin/not-found";
import { SiteBody, VIEWPORT, htmlClass, siteMetadata } from "@/components/SiteBody";
import "./globals.css";

// The 404 page (out/404.html, which Cloudflare Pages serves for any missing path). With two root layouts,
// app/(en)/ and app/[locale]/, a 404 needs its own (next.config.ts experimental.globalNotFound). In English.
export const metadata: Metadata = siteMetadata("en");
export const viewport = VIEWPORT;

export default function GlobalNotFound() {
  return (
    <html lang="en" className={htmlClass}>
      <SiteBody locale="en"><NotFound /></SiteBody>
    </html>
  );
}
