import type { Metadata } from "next";
import { ChallengeInvite } from "@/components/ChallengeInvite";
import { STASH_SCRIPT } from "@/lib/challenge";
import { SHARE_IMAGE } from "@/lib/site";
import { downloadText } from "@/lib/i18n";

// A link preview fetches /c/ without its fragment, so the preview is the general invite.
export const metadata: Metadata = {
  title: "Can you beat my score?",
  description: "A friend has challenged you to sing a song on OnKey, a free singing trainer for the Mac. Can you beat their score?",
  robots: { index: false, follow: true },
  openGraph: { title: "Can you beat my score?", description: "A singing challenge on OnKey, a free singing trainer for the Mac.", url: "/c/", siteName: "OnKey", type: "website", images: [{ url: SHARE_IMAGE, width: 1280, height: 640, alt: "OnKey: learn to sing the songs you love" }] },
};

/** "Beat my score" (lib/challenge.ts): the challenge is read in the browser, from the link's fragment. */
export default function ChallengePage() {
  return (
    <>
      {/* first: the challenge out of the address before analytics scripts load (a fixed script, no link data in it) */}
      <script dangerouslySetInnerHTML={{ __html: STASH_SCRIPT }} />
      <ChallengeInvite download={downloadText("en")} />
    </>
  );
}
