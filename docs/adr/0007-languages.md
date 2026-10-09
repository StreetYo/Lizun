# ADR 0007: Interface languages

Status: accepted (2026-10-10).

## Context

Lizun is open source and its users are not only English speakers; the author's own language is
Russian. Every text was written into the code in English: in the Svelte components, in the
settings window, in the Rust core (the tray menu, the settings window's title, the calendar's
settings and the errors of the calendar feed, of widget installs and of the browser extension's
port) and in the bundled To-do widget. Third-party widgets had no way to know the user's
language or to translate their name and settings.

## Decision

- **One JSON file per language** in `apps/desktop/src/locales/<code>.json`, such as `en.json`
  and `ru.json`, holding every text of the island, the settings window and the core, nested by
  area (`settings.general.language`). A translator works in one file and needs no code.
  `languageName` is the language's name in itself, shown in the language list.
- **English is the source.** It is complete by definition, and any text missing from a
  translation comes from it, so a partial translation still works.
- **The core reads the same files.** `src-tauri/src/i18n.rs` embeds them with `include_str!`
  and offers `t(key)` and `tf(key, &[("name", &value)])`. The web UI picks the files up with
  `import.meta.glob`; `t(key, params)` in `src/lib/i18n.svelte.ts` is typed with the keys of
  `en.json`, so a misspelt key fails `svelte-check`.
- **Placeholders are `{name}`**, filled in one pass. **Plurals** follow the CLDR rules through
  `Intl.PluralRules`: a text has forms such as `dropFiles_one`, `dropFiles_few`,
  `dropFiles_many` and `dropFiles_other`, and `t('island.dropFiles', { count })` picks the
  form. Russian needs three forms (1 файл, 3 файла, 5 файлов) plus `other` for fractions. The core has no plural texts.
- **Text with markup** (a bold port number inside a sentence) is split around its placeholder
  with `tSplit`, so translators still see and reorder the whole sentence.
- **The `language` preference** is a language code or `system` (the default). `system` takes
  the first of the user's Windows display languages (`GetUserPreferredUILanguages`) that Lizun
  has, matching `ru-RU` to `ru`, and falls back to English. The core resolves it, since it needs
  a language for the tray menu before any window exists, and reports it through
  `i18n_system_language`. A preference naming a language Lizun no longer has turns back into
  `system`.
- **Switching takes effect at once.** The new preference is broadcast like every other one: the
  windows re-render, the core relabels the tray menu and the settings window's title, and the
  settings window reads the built-in widgets' settings again, since the core words them. Windows
  wait for the language before their first render, so they never flash English. Errors already
  shown (a calendar feed that failed, a busy port) stay in the old language until they happen
  again.
- **Dates and times** are formatted with `Intl` in the interface language rather than in English.
- **Widgets get the language** in `ready()` (`context.language`) and through
  `language.subscribe()`; the SDK also sets the page's `lang`. Texts in the manifest are
  translated by the island: a manifest can carry `locales`, with `name`, `description` and
  per-setting `label`, `description`, `placeholder`, `unit` and option labels for each language
  code. The core checks them against the settings with the same limits. The bundled To-do
  widget is translated this way and serves as the example.
- **Checks.** A Rust test fails when a file in `locales` is not listed in `i18n.rs`, when a
  translation lacks a text English has, has a text English does not have, or uses a
  placeholder the English text does not.

## Alternatives considered

| Option | Why not |
|---|---|
| A library (i18next, svelte-i18n, Paraglide) | Each brings its own format and build step for what is a few dozen lines here, and none of them is read by the Rust core, which would need a second set of files. |
| Fluent (`.ftl`) | Handles grammar better than `_one`/`_few` keys, but needs a runtime in both TypeScript and Rust, and fewer translators know it than plain JSON. |
| Separate files for the core | The core's texts are a small part, and two sets of files per language would drift apart. |
| Translating the core's errors in the web UI by error code | Every error path in Rust would turn into a code plus arguments; translating the message where it is made keeps the code as it was. |
| Following only the language of the WebView | The core needs the language before any WebView runs, and both must agree. |

## Consequences

- Adding a language is a new file in `locales` and one line in `SOURCES` in `i18n.rs`; nothing
  else changes. The language list in Settings > General shows it by itself.
- Messages a developer reads (manifest validation details, logs) stay in English.
- Names that come from elsewhere stay as they are: apps and browsers, the titles of tracks and
  events.
- Lizun does not translate a widget's own interface; the widget does, from `context.language`.
