import type { Metadata } from "next";
import { CreditsPage, creditsTitle } from "@/components/pages/Credits";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: creditsTitle("en"), ...alternates("/docs/credits/") };

export default function Credits() {
  return <CreditsPage locale="en" />;
}
