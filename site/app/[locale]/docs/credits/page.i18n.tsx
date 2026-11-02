import type { Metadata } from "next";
import { CreditsPage, creditsTitle } from "@/components/pages/Credits";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: creditsTitle((await params).locale), ...alternates("/docs/credits/") };
}

export default async function Credits({ params }: Props) {
  return <CreditsPage locale={(await params).locale} />;
}
