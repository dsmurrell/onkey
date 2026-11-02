import type { Metadata } from "next";
import { TroubleshootingPage, troubleshootingTitle } from "@/components/pages/Troubleshooting";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: troubleshootingTitle("en"), ...alternates("/docs/troubleshooting/") };

export default function Troubleshooting() {
  return <TroubleshootingPage locale="en" />;
}
