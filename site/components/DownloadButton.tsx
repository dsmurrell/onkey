"use client";
import { useEffect, useState } from "react";
import { DOWNLOAD_URL, SITE_URL } from "@/lib/site";
import { track } from "@/lib/track";
import { FM_EVENTS, fm } from "@/lib/flowmyna";

/** What the visitor is on: a Mac (download), a phone or tablet (send the link to their Mac), or another computer
 *  (a note that it runs on Macs only for now). The page is static, so it's read in the browser; until then (and with no script) it's a Mac,
 *  the plain download link. iPads ask for desktop sites as a Mac, so a Mac with a touch screen is a tablet. */
type Device = "mac" | "mobile" | "other";
function detect(): Device {
  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod|Android|Mobile/i.test(ua)) return "mobile";
  if (/Macintosh/i.test(ua)) return navigator.maxTouchPoints > 1 ? "mobile" : "mac";
  return "other";
}

export function useDevice(): Device {
  const [d, setD] = useState<Device>("mac");
  useEffect(() => { setD(detect()); }, []);
  return d;
}

const LINK = SITE_URL ?? "https://singonkey.app";

/** The page's main call to action, for whatever the visitor is on. `note`: show the line under it (not in the header). */
/** `where`: which button it is (header, top, bottom), kept with the visit-path event. */
/** `text`: the button's words in the page's language (lib/i18n.tsx downloadText), as this runs in the browser. */
export type DownloadText = { mac: string; macsOnly: React.ReactNode; send: string; sent: string; phones: string; shareText: string; mailSubject: string };

export function DownloadButton({ className, note = true, where, text }: { className: string; note?: boolean; where: string; text: DownloadText }) {
  const device = useDevice();
  const [sent, setSent] = useState("");
  if (device === "mac") return <a href={DOWNLOAD_URL} data-track="Download for Mac" onClick={() => track("Download")} className={className}>{text.mac}</a>;
  if (device === "other") {
    return (
      <span className="inline-grid gap-1.5">
        <a href={DOWNLOAD_URL} data-track="Download (not a Mac)" onClick={() => track("Download (not a Mac)")} className={className}>{text.mac}</a>
        {note && <span className="text-xs text-muted">{text.macsOnly}</span>}
      </span>
    );
  }
  // a phone or tablet: the share sheet (AirDrop, Messages, a note to self), else an email to yourself
  const send = async () => {
    track("Send to Mac");
    fm(FM_EVENTS.sentToMac, { button: where });  // (downloads and other ways out: Flow Myna's clickout autocapture)
    const data = { title: "OnKey", text: text.shareText, url: LINK };
    if (navigator.share) {
      try { await navigator.share(data); setSent(text.sent); } catch { /* closed the sheet */ }
      return;
    }
    location.href = `mailto:?subject=${encodeURIComponent(text.mailSubject)}&body=${encodeURIComponent(`${data.text} ${LINK}`)}`;
  };
  return (
    <span className="inline-grid gap-1.5">
      <button type="button" onClick={send} className={className}>{sent || text.send}</button>
      {note && <span className="text-xs text-muted">{text.phones}</span>}
    </span>
  );
}
