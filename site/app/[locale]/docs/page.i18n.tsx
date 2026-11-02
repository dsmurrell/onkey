import type { Metadata } from "next";
import { GettingStartedPage, startTitle } from "@/components/pages/GettingStarted";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: startTitle((await params).locale), ...alternates("/docs/") };
}

export default async function GettingStarted({ params }: Props) {
  return <GettingStartedPage locale={(await params).locale} />;
}
