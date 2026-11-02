import type { Metadata } from "next";
import { TroubleshootingPage, troubleshootingTitle } from "@/components/pages/Troubleshooting";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: troubleshootingTitle((await params).locale), ...alternates("/docs/troubleshooting/") };
}

export default async function Troubleshooting({ params }: Props) {
  return <TroubleshootingPage locale={(await params).locale} />;
}
