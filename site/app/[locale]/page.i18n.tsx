import { HomePage } from "@/components/pages/Home";
import { alternates } from "@/lib/i18n";

export const metadata = alternates("/");

export default async function Home({ params }: { params: Promise<{ locale: string }> }) {
  return <HomePage locale={(await params).locale} />;
}
