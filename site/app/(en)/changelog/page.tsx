import type { Metadata } from "next";
import { ChangelogPage, changelogMetadata } from "@/components/pages/Changelog";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { ...changelogMetadata("en"), ...alternates("/changelog/") };

export default function Changelog() {
  return <ChangelogPage locale="en" />;
}
