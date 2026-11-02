import type { Metadata } from "next";
import { PrivacyPage, privacyTitle } from "@/components/pages/Privacy";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: privacyTitle((await params).locale), ...alternates("/docs/privacy/") };
}

export default async function Privacy({ params }: Props) {
  return <PrivacyPage locale={(await params).locale} />;
}
