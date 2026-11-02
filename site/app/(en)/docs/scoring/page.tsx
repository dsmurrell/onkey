import type { Metadata } from "next";
import { ScoringPage, scoringTitle } from "@/components/pages/Scoring";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: scoringTitle("en"), ...alternates("/docs/scoring/") };

export default function Scoring() {
  return <ScoringPage locale="en" />;
}
