import type { Metadata } from "next";
import { GettingStartedPage, startTitle } from "@/components/pages/GettingStarted";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: startTitle("en"), ...alternates("/docs/") };

export default function GettingStarted() {
  return <GettingStartedPage locale="en" />;
}
