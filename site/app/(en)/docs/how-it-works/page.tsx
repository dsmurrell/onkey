import type { Metadata } from "next";
import { HowItWorksPage, howItWorksTitle } from "@/components/pages/HowItWorks";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: howItWorksTitle("en"), ...alternates("/docs/how-it-works/") };

export default function HowItWorks() {
  return <HowItWorksPage locale="en" />;
}
