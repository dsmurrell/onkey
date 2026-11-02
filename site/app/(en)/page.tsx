import { HomePage } from "@/components/pages/Home";
import { alternates } from "@/lib/i18n";

export const metadata = alternates("/");

export default function Home() {
  return <HomePage locale="en" />;
}
