import type { Metadata } from "next";
import { HowItWorksPage, howItWorksTitle } from "@/components/pages/HowItWorks";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: howItWorksTitle((await params).locale), ...alternates("/docs/how-it-works/") };
}

export default async function HowItWorks({ params }: Props) {
  return <HowItWorksPage locale={(await params).locale} />;
}
