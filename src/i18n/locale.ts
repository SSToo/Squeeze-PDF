import { catalog, type MessageKey } from "./catalog.ts";

export const languages = [
  { id: "en", name: "En", tag: "en" },
  { id: "zh", name: "中文", tag: "zh-CN" },
  { id: "fr", name: "Français", tag: "fr" },
  { id: "es", name: "Español", tag: "es" },
  { id: "ko", name: "한국어", tag: "ko" },
  { id: "ja", name: "日本語", tag: "ja" },
  { id: "de", name: "Deutsch", tag: "de" },
  { id: "pt", name: "Português", tag: "pt" },
] as const;
export type Language = (typeof languages)[number]["id"];
export type Values = Record<string, string | number>;
const columns = { en: 0, fr: 1, es: 2, ko: 3, ja: 4, de: 5, pt: 6 } as const;
const numeric = new Set([
  "count",
  "page",
  "pages",
  "images",
  "total",
  "done",
  "ok",
  "failed",
  "streams",
  "width",
  "height",
  "limit",
  "object",
  "generation",
]);
const nested = new Set(["detail", "encoding", "resolution", "sampling"]);
export function normalizeLanguage(
  value: string | null | undefined,
): Language | undefined {
  const base = value?.toLowerCase().split(/[-_]/)[0];
  return languages.find((language) => language.id === base)?.id;
}
export function detectLanguage(
  saved: string | null | undefined,
  preferred: readonly string[],
): Language {
  return (
    normalizeLanguage(saved) ??
    preferred.map(normalizeLanguage).find(Boolean) ??
    "en"
  );
}
const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
// Legacy Rust diagnostics remain unchanged on the wire. Match whole messages,
// never replace substrings in arbitrary file names or technical details.
const patterns = (Object.keys(catalog) as MessageKey[])
  .filter((key) => key.includes("{"))
  .map((key) => {
    const names: string[] = [];
    const parts = key.split(/(\{[a-z]+\})/g);
    const source = parts
      .map((part) => {
        const name = /^\{([a-z]+)\}$/.exec(part)?.[1];
        if (!name) return escape(part);
        names.push(name);
        return name === "sampling"
          ? "(，超采样抗锯齿)?"
          : numeric.has(name)
            ? "(\\d+(?:\\.\\d+)?)"
            : "([\\s\\S]+?)";
      })
      .join("");
    return {
      key,
      names,
      prefix: parts[0],
      regex: new RegExp("^" + source + "$"),
    };
  })
  .sort((a, b) => b.prefix.length - a.prefix.length);

export function createTranslator(language: Language) {
  const tag = languages.find((item) => item.id === language)!.tag;
  const formats = new Map<number, Intl.NumberFormat>();
  const number = (value: number, decimals = 0) => {
    if (!formats.has(decimals))
      formats.set(
        decimals,
        new Intl.NumberFormat(tag, {
          minimumFractionDigits: decimals,
          maximumFractionDigits: decimals,
        }),
      );
    return formats.get(decimals)!.format(value);
  };
  const t = (key: MessageKey, values: Values = {}) => {
    const text = language === "zh" ? key : catalog[key][columns[language]];
    return text.replace(/\{([a-z]+)\}/g, (token, name: string) => {
      const value = values[name];
      return value === undefined
        ? token
        : typeof value === "number"
          ? number(value)
          : value;
    });
  };
  const raw = (message: string | null | undefined, depth = 0): string => {
    if (!message) return "";
    const source = message.trim();
    if (language === "zh" || depth > 8) return source;
    const errorPrefix = /^(Error|TypeError|RangeError):\s*/.exec(source);
    if (errorPrefix)
      return (
        errorPrefix[0] + raw(source.slice(errorPrefix[0].length), depth + 1)
      );
    if (Object.hasOwn(catalog, source)) return t(source as MessageKey);
    for (const pattern of patterns) {
      if (!source.startsWith(pattern.prefix)) continue;
      const match = pattern.regex.exec(source);
      if (!match) continue;
      const values: Values = {};
      pattern.names.forEach((name, i) => {
        const value = match[i + 1] ?? "";
        values[name] = nested.has(name)
          ? raw(value, depth + 1)
          : numeric.has(name)
            ? Number(value)
            : value;
      });
      return t(pattern.key, values);
    }
    // Worker stderr can contain more than one diagnostic. Keep unknown library
    // or OS messages verbatim rather than inventing or hiding technical facts.
    if (source.includes("\n"))
      return source
        .split("\n")
        .map((line) => raw(line, depth + 1))
        .join("\n");
    return source;
  };
  const bytes = (value: number) =>
    value >= 1048576
      ? number(value / 1048576, 2) + (language === "fr" ? " Mo" : " MB")
      : number(value / 1024, 1) + (language === "fr" ? " Ko" : " KB");
  const percentFormats = new Map<number, Intl.NumberFormat>();
  const percent = (value: number, decimals = 0) => {
    if (!percentFormats.has(decimals))
      percentFormats.set(
        decimals,
        new Intl.NumberFormat(tag, {
          style: "percent",
          minimumFractionDigits: decimals,
          maximumFractionDigits: decimals,
        }),
      );
    return percentFormats.get(decimals)!.format(value);
  };
  return { t, raw, number, bytes, percent };
}
