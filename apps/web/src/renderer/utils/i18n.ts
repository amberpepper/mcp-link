import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import type { AppLanguage } from "@mcp_link/shared";
import enTranslation from "../../locales/en.json";
import jaTranslation from "../../locales/ja.json";
import zhTranslation from "../../locales/zh.json";

const supportedAppLanguages: AppLanguage[] = ["en", "zh", "ja"];

export function normalizeAppLanguage(language?: string | null): AppLanguage {
  const normalizedLanguage = language?.toLowerCase();

  if (normalizedLanguage?.startsWith("zh")) return "zh";
  if (normalizedLanguage?.startsWith("ja")) return "ja";
  if (normalizedLanguage?.startsWith("en")) return "en";

  return "en";
}

// Initialize i18next
i18n
  // Detect user language
  .use(LanguageDetector)
  // Pass the i18n instance to react-i18next
  .use(initReactI18next)
  // Set up i18next
  .init({
    resources: {
      en: {
        translation: enTranslation,
      },
      ja: {
        translation: jaTranslation,
      },
      zh: {
        translation: zhTranslation,
      },
    },
    fallbackLng: "en",
    supportedLngs: supportedAppLanguages,
    nonExplicitSupportedLngs: true,
    // Debug mode in development environment
    debug: import.meta.env.DEV,

    // Common namespace used around the app
    ns: ["translation"],
    defaultNS: "translation",

    // Load zh resources for zh-CN/zh-TW, ja resources for ja-JP, etc.
    load: "languageOnly",

    interpolation: {
      escapeValue: false, // React already safes from XSS
    },

    // Allow returning objects from translation keys
    returnObjects: true,

    // React settings
    react: {
      useSuspense: true,
    },
  });
