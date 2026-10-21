import i18next, { type i18n } from "i18next";
import { initReactI18next } from "react-i18next";
import zhCommon from "../locales/zh-TW/common.json";
import zhOnboarding from "../locales/zh-TW/onboarding.json";
import zhImport from "../locales/zh-TW/import.json";
import zhSettings from "../locales/zh-TW/settings.json";
import zhSearch from "../locales/zh-TW/search.json";
import zhSql from "../locales/zh-TW/sql.json";
import enCommon from "../locales/en/common.json";
import enOnboarding from "../locales/en/onboarding.json";
import enImport from "../locales/en/import.json";
import enSettings from "../locales/en/settings.json";
import enSearch from "../locales/en/search.json";
import enSql from "../locales/en/sql.json";

export const NAMESPACES = ["common", "onboarding", "import", "settings", "search", "sql"] as const;
export type Namespace = (typeof NAMESPACES)[number];

export const RESOURCES: Record<"zh-TW" | "en", Record<Namespace, Record<string, unknown>>> = {
  "zh-TW": { common: zhCommon, onboarding: zhOnboarding, import: zhImport, settings: zhSettings, search: zhSearch, sql: zhSql },
  en: { common: enCommon, onboarding: enOnboarding, import: enImport, settings: enSettings, search: enSearch, sql: enSql },
};

/** Synchronous init: resources are bundled, so `init` loads them before it returns. */
export function createI18n(lng: string): i18n {
  const instance = i18next.createInstance();
  void instance.use(initReactI18next).init({
    lng,
    fallbackLng: "en",
    ns: [...NAMESPACES],
    defaultNS: "common",
    resources: RESOURCES,
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  return instance;
}
