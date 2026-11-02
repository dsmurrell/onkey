import type { Metadata } from "next";
import { FaqPage, faqTitle } from "@/components/pages/Faq";
import { alternates } from "@/lib/i18n";

export const metadata: Metadata = { title: faqTitle("en"), ...alternates("/docs/faq/") };

export default function Faq() {
  return <FaqPage locale="en" />;
}
