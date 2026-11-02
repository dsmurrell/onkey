import type { Metadata } from "next";
import { BeyondPitchPage, beyondPitchTitle } from "@/components/pages/BeyondPitch";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: beyondPitchTitle((await params).locale), ...alternates("/docs/beyond-pitch/") };
}

export default async function BeyondPitch({ params }: Props) {
  return <BeyondPitchPage locale={(await params).locale} />;
}
