import type { Metadata } from "next";
import { BeyondPitchPage, beyondPitchTitle } from "@/components/pages/BeyondPitch";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: beyondPitchTitle("en"), ...alternates("/docs/beyond-pitch/") };

export default function BeyondPitch() {
  return <BeyondPitchPage locale="en" />;
}
