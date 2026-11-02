import type { Metadata } from "next";
import { FaqPage, faqTitle } from "@/components/pages/Faq";
import { alternates } from "@/lib/i18n";

type Props = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  return { title: faqTitle((await params).locale), ...alternates("/docs/faq/") };
}

export default async function Faq({ params }: Props) {
  return <FaqPage locale={(await params).locale} />;
}
