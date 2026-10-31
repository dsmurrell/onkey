import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import ControlPrompt from "@/components/ControlPrompt";
import ReportProblem from "@/components/ReportProblem";
import MirrorBadge from "@/components/MirrorBadge";
import UpdatePill from "@/components/UpdatePill";
import GoHome from "@/components/GoHome";
import DevDrive from "@/components/DevDrive";
import OpenLinks from "@/components/OpenLinks";

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({ variable: "--font-geist-mono", subsets: ["latin"] });

export const metadata: Metadata = {
  title: "OnKey",
  description: "A singing trainer: learn to sing the songs you love, with the music you already play.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body className={`${geistSans.variable} ${geistMono.variable} antialiased`}>
        <script dangerouslySetInnerHTML={{ __html: "if ('__TAURI_INTERNALS__' in window) { document.documentElement.classList.add('in-app'); const log = m => { try { window.__TAURI_INTERNALS__.invoke('ui_log', { msg: String(m).slice(0, 2000) }); } catch (e) {} }; window.addEventListener('error', e => log(e.message + ' at ' + e.filename + ':' + e.lineno)); window.addEventListener('unhandledrejection', e => log('unhandled: ' + (e.reason && (e.reason.stack || e.reason)))); const ce = console.error; console.error = (...a) => { log(a.join(' ')); ce(...a); }; window.addEventListener('load', () => log('window loaded')); }" }} />
        {children}
        <ControlPrompt />
        <ReportProblem />
        <MirrorBadge />
        <UpdatePill />
        <GoHome />
        <DevDrive />
        <OpenLinks />
      </body>
    </html>
  );
}
