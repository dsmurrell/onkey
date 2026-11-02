"use client";
// The window's words for every component under it (use-intl's provider): the language shown (lib/i18n.ts; English
// while the page is built) and its messages. `useT()` reads them, and a change of language re-renders.
import type { ReactNode } from "react";
import { IntlProvider } from "use-intl/react";
import { getMessageFallback, messagesFor, onIntlError, timeZone, useLocale } from "@/lib/i18n";

export default function I18n({ children }: { children: ReactNode }) {
  const locale = useLocale();
  return (
    <IntlProvider locale={locale} messages={messagesFor(locale)} onError={onIntlError} getMessageFallback={getMessageFallback} timeZone={timeZone}>
      {children}
    </IntlProvider>
  );
}
