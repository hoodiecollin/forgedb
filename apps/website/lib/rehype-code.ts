import { createHighlighter, type LanguageRegistration } from "shiki";
import type { Options as PrettyCodeOptions } from "rehype-pretty-code";
import forgeGrammar from "@/lib/forge.tmLanguage.json";

const forgeLang = {
  ...(forgeGrammar as unknown as LanguageRegistration),
  name: "forge",
  aliases: ["forgedb", "schema"],
} as LanguageRegistration;
const BUILTIN_LANGS = [
  "bash",
  "rust",
  "typescript",
  "tsx",
  "javascript",
  "json",
  "toml",
  "yaml",
  "sql",
  "diff",
  "text",
] as const;
export const rehypePrettyCodeOptions: PrettyCodeOptions = {
  theme: { light: "github-light", dark: "github-dark" },
  keepBackground: false,
  defaultLang: { block: "text", inline: "text" },
  getHighlighter: (options) =>
    createHighlighter({
      ...options,
      langs: [...BUILTIN_LANGS, forgeLang],
    }),
};
