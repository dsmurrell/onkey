import type { Metadata } from "next";
import { YourVoicePage, yourVoiceTitle } from "@/components/pages/YourVoice";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: yourVoiceTitle((await params).locale), ...alternates("/docs/your-voice/") };
}

export default async function YourVoice({ params }: Props) {
  return <YourVoicePage locale={(await params).locale} />;
}
