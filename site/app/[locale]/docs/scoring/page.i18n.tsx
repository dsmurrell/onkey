import type { Metadata } from "next";
import { ScoringPage, scoringTitle } from "@/components/pages/Scoring";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: scoringTitle((await params).locale), ...alternates("/docs/scoring/") };
}

export default async function Scoring({ params }: Props) {
  return <ScoringPage locale={(await params).locale} />;
}
