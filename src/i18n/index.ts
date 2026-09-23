import i18n from "i18next"
import { initReactI18next } from "react-i18next"

import en from "./en.json"
import zh from "./zh.json"

export type Language = "en" | "zh"

const STORAGE_KEY = "language"

export function getLanguage(): Language {
  const value = localStorage.getItem(STORAGE_KEY)
  return value === "zh" ? "zh" : "en"
}

export function setLanguage(lang: Language) {
  localStorage.setItem(STORAGE_KEY, lang)
  void i18n.changeLanguage(lang)
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
  },
  lng: getLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
})

export default i18n
