/**
 * Interface language (see docs/adr/0007-languages.md). Every language is a file in
 * src/locales, shared with the core; English is the source and fills in for any text a
 * translation lacks. The files are picked up here by themselves; the core lists them in
 * src-tauri/src/i18n.rs.
 */
import { untrack } from 'svelte';
import type en from '../locales/en.json';
import { invoke } from './platform';
import { preferences } from './preferences.svelte';

const FALLBACK = 'en';
const PLURAL_FORMS = ['zero', 'one', 'two', 'few', 'many', 'other'] as const;

type PluralForm = (typeof PLURAL_FORMS)[number];
type BaseKey<K extends string> = K extends `${infer Base}_${PluralForm}` ? Base : K;
type Keys<T, Prefix extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? `${Prefix}${BaseKey<K>}` : Keys<T[K], `${Prefix}${K}.`>;
}[keyof T & string];

/** A text's dotted key, such as `settings.nav.general`; plural texts go by the key without the form. */
export type MessageKey = Keys<typeof en>;
export type MessageParams = Record<string, string | number>;

export interface Language {
  code: string;
  /** The language's name in itself, such as "Русский". */
  name: string;
}

const files = import.meta.glob<Record<string, unknown>>('../locales/*.json', { eager: true, import: 'default' });

/** Texts of each language by dotted key. */
const catalogs = new Map<string, Map<string, string>>(
  Object.entries(files).map(([path, texts]) => [path.slice(path.lastIndexOf('/') + 1, -'.json'.length), flatten(texts)]),
);

function flatten(value: unknown, prefix = '', texts = new Map<string, string>()): Map<string, string> {
  if (typeof value === 'string') texts.set(prefix, value);
  else if (value && typeof value === 'object') {
    for (const [name, child] of Object.entries(value)) flatten(child, prefix ? `${prefix}.${name}` : name, texts);
  }
  return texts;
}

/** Every translation with its own name, in a fixed order. */
export const LANGUAGES: Language[] = [...catalogs]
  .map(([code, texts]) => ({ code, name: texts.get('languageName') ?? code }))
  .sort((a, b) => a.code.localeCompare(b.code));

class I18n {
  /** The language Windows would pick, as the core reports it. */
  system = $state(FALLBACK);

  #started: Promise<void> | null = null;
  #listeners = new Set<(language: string) => void>();

  /** The language in use: the one chosen in the settings, or the system's. */
  get language(): string {
    const chosen = preferences.value.language;
    return catalogs.has(chosen) ? chosen : this.system;
  }

  /** Loads the preferences and the system language; call it before the first render. */
  start(): Promise<void> {
    this.#started ??= (async () => {
      const [, system] = await Promise.all([
        preferences.start(),
        invoke<string>('i18n_system_language').catch(() => FALLBACK),
      ]);
      if (system && catalogs.has(system)) this.system = system;
      let previous = '';
      $effect.root(() => {
        $effect(() => {
          const language = this.language;
          if (language === previous) return;
          previous = language;
          document.documentElement.lang = language;
          untrack(() => this.#listeners.forEach((listener) => listener(language)));
        });
      });
    })();
    return this.#started;
  }

  /** Called with the new language whenever it changes. */
  onChange(listener: (language: string) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }
}

export const i18n = new I18n();

const pluralRules = new Map<string, Intl.PluralRules>();

function pluralForm(language: string, count: number): PluralForm {
  let rules = pluralRules.get(language);
  if (!rules) pluralRules.set(language, (rules = new Intl.PluralRules(language)));
  return rules.select(count);
}

/** The text in the current language. A numeric `count` picks the plural form, such as
 * `files_few` for 3 in Russian; `{name}` placeholders take the values in `params`. */
export function t(key: MessageKey, params?: MessageParams): string {
  const language = i18n.language;
  const count = params?.count;
  let text: string | undefined;
  for (const code of [language, FALLBACK]) {
    const texts = catalogs.get(code);
    if (!texts) continue;
    text =
      typeof count === 'number'
        ? (texts.get(`${key}_${pluralForm(code, count)}`) ?? texts.get(`${key}_other`) ?? texts.get(key))
        : texts.get(key);
    if (text !== undefined) break;
  }
  if (text === undefined) {
    console.warn(`no text for ${key}`);
    return key;
  }
  return params ? text.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match)) : text;
}

/** The text split around one placeholder, for markup such as bold text in its place. */
export function tSplit(key: MessageKey, placeholder: string, params?: MessageParams): [string, string] {
  const text = t(key, params);
  const at = text.indexOf(`{${placeholder}}`);
  return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + placeholder.length + 2)];
}

/** Picks the entry for a language from texts keyed by language code: the same code, or one
 * for the same language in another region (`pt-BR` for `pt`). */
export function forLanguage<T>(byCode: Record<string, T> | undefined, language = i18n.language): T | undefined {
  if (!byCode) return undefined;
  if (language in byCode) return byCode[language];
  const primary = (code: string) => code.split('-')[0].toLowerCase();
  const match = Object.keys(byCode).find((code) => primary(code) === primary(language));
  return match === undefined ? undefined : byCode[match];
}
