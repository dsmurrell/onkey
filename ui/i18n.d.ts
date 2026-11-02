// Typed message keys: use-intl checks every t("…") against English's catalog (messages/en.json), so a key spelled
// wrong, or one that's gone, fails `tsc`.
import type en from "./messages/en.json";

declare module "use-intl" {
  interface AppConfig {
    Messages: typeof en;
  }
}
