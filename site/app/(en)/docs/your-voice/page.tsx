import type { Metadata } from "next";
import { YourVoicePage, yourVoiceTitle } from "@/components/pages/YourVoice";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: yourVoiceTitle("en"), ...alternates("/docs/your-voice/") };

export default function YourVoice() {
  return <YourVoicePage locale="en" />;
}
