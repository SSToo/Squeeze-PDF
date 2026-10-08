import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  createTranslator,
  detectLanguage,
  languages,
  type Language,
} from "./locale";

function initialLanguage(): Language {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem("squeeze-language");
  } catch {
    /* storage can be unavailable */
  }
  return detectLanguage(
    saved,
    navigator.languages?.length ? navigator.languages : [navigator.language],
  );
}
const I18nContext = createContext<
  | (ReturnType<typeof createTranslator> & {
      language: Language;
      setLanguage: (language: Language) => void;
    })
  | null
>(null);
export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguage] = useState<Language>(initialLanguage);
  const value = useMemo(
    () => ({ language, setLanguage, ...createTranslator(language) }),
    [language],
  );
  useEffect(() => {
    document.documentElement.lang = languages.find(
      (item) => item.id === language,
    )!.tag;
    document.title = value.t("Squeeze PDF · 本地 PDF 压缩");
    try {
      localStorage.setItem("squeeze-language", language);
    } catch {
      /* preference is optional */
    }
  }, [language, value]);
  return <I18nContext value={value}>{children}</I18nContext>;
}
export function useI18n() {
  const value = useContext(I18nContext);
  if (!value) throw new Error("I18nProvider is missing");
  return value;
}
