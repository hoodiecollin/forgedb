import { createHighlighter, type LanguageRegistration } from "shiki";
import type { Options as PrettyCodeOptions } from "rehype-pretty-code";
import forgeGrammar from "@/lib/forge.tmLanguage.json";

/**
 * Register the `.forge` schema language from the VS Code extension's own
 * TextMate grammar (`vscode-forgedb/syntaxes/forge.tmLanguage.json`), so
 * ```forge fenced blocks highlight authentically instead of falling back to
 * plaintext. Shiki uses the registration's `name` as the language id.
 */
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

/** rehype-pretty-code options wired with dual light/dark themes + `.forge`. */
export const rehypePrettyCodeOptions: PrettyCodeOptions = {
  theme: { light: "github-light", dark: "github-dark" },
  keepBackground: false,
  defaultLang: { block: "text", inline: "text" },
  // Reuse one highlighter instance across the whole build; register our langs.
  getHighlighter: (options) =>
    createHighlighter({
      ...options,
      langs: [...BUILTIN_LANGS, forgeLang],
    }),
};
