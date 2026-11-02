import type { Metadata } from "next";
import { PrivacyPage, privacyTitle } from "@/components/pages/Privacy";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: privacyTitle("en"), ...alternates("/docs/privacy/") };

export default function Privacy() {
  return <PrivacyPage locale="en" />;
}
