import { DocsLayout } from "@/components/DocsLayout";

export default async function Layout({ children, params }: { children: React.ReactNode; params: Promise<{ locale: string }> }) {
  return <DocsLayout locale={(await params).locale}>{children}</DocsLayout>;
}
