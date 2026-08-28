import { createHighlighter, type Highlighter, type LanguageRegistration } from "shiki";
import forgeGrammar from "@/lib/forge.tmLanguage.json";

const forgeLang = {
  ...(forgeGrammar as unknown as LanguageRegistration),
  name: "forge",
} as LanguageRegistration;

let hp: Promise<Highlighter> | null = null;

function getHighlighter(): Promise<Highlighter> {
  hp ??= createHighlighter({
    themes: ["github-light", "github-dark"],
    langs: ["bash", "rust", "typescript", "tsx", "python", "go", "json", "toml", "sql", forgeLang],
  });
  return hp;
}

export async function highlight(code: string, lang: string): Promise<string> {
  const h = await getHighlighter();
  const known = h.getLoadedLanguages().includes(lang) ? lang : "text";
  return h.codeToHtml(code, {
    lang: known,
    themes: { light: "github-light", dark: "github-dark" },
    defaultColor: false,
  });
}
