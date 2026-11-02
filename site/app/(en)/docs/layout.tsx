import { DocsLayout } from "@/components/DocsLayout";

export default function Layout({ children }: { children: React.ReactNode }) {
  return <DocsLayout locale="en">{children}</DocsLayout>;
}
