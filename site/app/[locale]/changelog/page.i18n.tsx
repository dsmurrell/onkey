import type { Metadata } from "next";
import { ChangelogPage, changelogMetadata } from "@/components/pages/Changelog";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { ...changelogMetadata((await params).locale), ...alternates("/changelog/") };
}

export default async function Changelog({ params }: Props) {
  return <ChangelogPage locale={(await params).locale} />;
}
