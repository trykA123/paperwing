# 46 — Syntax colouring for automotive and embedded files in compare

Status: waits for packet 37 step 1 (CodeMirror swap); retargeted from Monaco to CodeMirror 6 on 2026-10-07 (owner request 2026-10-07: "add support for more types of file extensions … automotive programming languages, .m4, .arxml, etc")
Platform: both
Size: M
Role: ui-builder (frontend only)

## Goal
Every file the owner compares at work gets sensible colouring: C and C++ with AUTOSAR headers, ARXML and other XML-based configs, m4 macros, A2L, DBC, LDF, CAPL, OIL, linker scripts and map files, S-record and Intel HEX, MATLAB and TLC, Makefiles, CMake, Python, shell and batch files. When Skein guesses wrong, the user picks the language from the compare toolbar and can map an extension once in Settings.

## Already done
- The compare editor is Monaco 0.57 (`src/lib/monaco.ts`, `src/lib/editor.ts`). `language(path)` maps about 15 extensions and returns `plaintext` for everything else.
- Packet 37 will replace Monaco with CodeMirror 6. This packet's registry must not depend on Monaco types, so 37 can reuse it.

## Retarget to CodeMirror 6 (alt, 2026-10-07)
Packet 37 step 1 replaces Monaco now, so this packet builds on CodeMirror 6 instead. Where the text below says Monaco:
- Registry entries are `{ id, label, load }`. `load()` returns a CodeMirror `LanguageSupport` or `StreamLanguage`, lazily imported. No Monaco ids or types.
- Built-in mappings use the official `@codemirror/lang-*` packages (cpp, xml, json, yaml, python, javascript, html, css, markdown, rust, sql, java, go) and `@codemirror/legacy-modes` (shell, powershell, perl, lua, properties/ini, cmake, octave for MATLAB), pinned exactly.
- Custom grammars (m4, asap2, dbc, ldf, capl, oil, linker, mapfile, srec, ihex, tlc) are `StreamLanguage` parsers in `src/lib/languages/<id>.ts`, mapped to the editor theme's highlight tags.
- Large files: the custom renderer from 37 has no colouring; that is acceptable.
- Run after 37 step 1 is merged; reuse its adapter (`src/lib/editor.ts`).

## Decisions
- **One registry.** `src/lib/languages.ts` is the single source.
  - It is keyed by lower-case extension, plus full file names (`Makefile`, `CMakeLists.txt`, `SConstruct`, `Jenkinsfile`).
  - Each entry is `{ id, label, monaco }`, where `monaco` is a built-in Monaco language id or a custom one registered by this packet.
  - First-line sniffing applies when the extension is unknown: `<?xml` gives XML, `#!` with bash, sh, python or perl gives that language, and an m4 `dnl` or `divert(` gives m4.
- **Built-in mappings (Monaco).**
  - C: `.c`, `.h`, `.i`.
  - C++: `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`, `.inl`.
  - XML: `.xml`, `.arxml`, `.xsd`, `.xsl`, `.xslt`, `.epc`, `.xdm`, `.cdd`, `.odx`, `.odx-d`, `.odx-c`, `.pdx`, `.cdfx`, `.fibex`, `.vsysvar`, `.svd`, `.launch`, `.cproject`, `.project`.
  - INI: `.ini`, `.cfg`, `.conf`, `.properties`, `.prefs`.
  - JSON: `.json`, `.jsonc`, `.json5`.
  - YAML: `.yml`, `.yaml`.
  - Other built-ins: `.py`, `.sh`, `.bash`, `.ps1`, `.bat`, `.cmd`, `.pl`, `.pm`, `.lua`, `.sql`, `.cs`, `.java`, `.go`, `.rs`, `.md`, `.html`, `.css`, `.js`, `.ts`, `.tsx`, `.jsx`.
  - Fallback: `plaintext`.
- **Custom grammars.** These are Monaco Monarch tokenizers in `src/lib/languages/<id>.ts`, small and data-driven, and they map onto the existing editor theme token classes.
  - `m4`: `.m4`, `.ac`, `configure.ac`. Covers `dnl` comments, quotes `` ` ' ``, `define`, `ifelse`, `include`, `divert` and `$1`-style arguments.
  - `asap2` (A2L): `.a2l`, `.aml`. Covers `/begin` and `/end` blocks, keywords (MEASUREMENT, CHARACTERISTIC, COMPU_METHOD, …), strings, numbers and comments.
  - `dbc`: `.dbc`. Covers BO_, SG_, BA_, VAL_, CM_ and other keywords, signal definitions, strings and numbers.
  - `ldf`: `.ldf`. Covers LIN description file keywords and blocks.
  - `capl`: `.can`, `.cin`. C keywords plus CAPL ones (`on message`, `on timer`, `variables`, `msTimer`, `output`, `write`).
  - `oil`: `.oil`. OSEK OIL keywords (CPU, OS, TASK, ISR, ALARM, EVENT, RESOURCE, COUNTER, APPMODE).
  - `linker`: `.ld`, `.lds`, `.lsl`, `.lcf`, `.icf`, `.x`. Covers SECTIONS, MEMORY, ENTRY, location counter, symbols and comments.
  - `mapfile`: `.map`, `.lst`. Addresses, section names and sizes.
  - `srec`: `.s19`, `.s28`, `.s37`, `.srec`, `.mot`. Covers the record type, byte count, address, data and checksum.
  - `ihex`: `.hex`, `.ihex`. Same fields as S-record.
  - `matlab`: `.m`, `.mlx` excluded. MATLAB keywords, `%` comments and strings.
  - `tlc`: `.tlc`. `%`-directives and `%%` comments.
  - `asm`: `.s`, `.S`, `.asm`, `.inc`. Generic assembler: labels, directives, registers and comments.
  - `makefile`: `Makefile`, `.mk`, `.mak`.
  - `cmake`: `CMakeLists.txt`, `.cmake`.
- **Binary formats** (`.slx`, `.mdl` binary, `.elf`, `.out`, `.bin`) stay on the binary path; do not colour them.
- **Manual override.** The compare toolbar gets a language picker showing the current label (for example "ARXML (XML)"). Choosing another language applies to that file now, and with "Use for all .ext files" it applies to that extension from then on.
- **User mappings.** Settings has an "Editor" section listing user extension mappings (extension → language) with add and remove. Persist them in the existing settings JSON as an optional `languageMap` field, so old settings files still load. User mappings override the built-in table.
- **Performance.** Register custom languages lazily on first use. Tokenizing a 5 MB file must not freeze the UI. If Monaco's large-file mode turns colouring off, keep that behaviour and show "Colouring off for large files".

## Scope
- Do: the registry and its tests; the custom Monarch grammars, each with a small fixture and a tokenizer test; the toolbar picker; the Settings mappings; wiring into `monaco.ts` and the file compare.
- Do not: switch to CodeMirror (packet 37); change compare diff logic; add a backend.

## Read first
`src/lib/monaco.ts`, `src/lib/editor.ts`, `src/components/FileCompare.svelte` and `file-compare/*` (Toolbar), `src/components/Settings.svelte` and `settings/*`, `src-tauri/src/settings.rs` (it only needs to accept the optional field; check whether the settings JSON is typed or a free `Value`), `~/.agents/rules/typescript.md`, `web-ui.md`, `code-quality.md`.

## Steps
1. The `languages.ts` registry and sniffing, with unit tests: every extension listed above, file names, sniffing, user override precedence.
2. Custom grammars, one file each, each with a fixture and a test of the token types on key lines.
3. Toolbar picker and the "use for all" option.
4. Settings "Editor" mappings, persisted. If the Rust settings struct rejects unknown fields, stop and report.
5. Browser check: compare fixture pairs for `.arxml`, `.m4`, `.a2l`, `.dbc`, `.can`, `.ld`, `.s19`, `.m` and `.c`, at 1440, in both themes. Screenshots.

## Done when
- Each listed type is coloured in compare.
- A wrong guess can be fixed from the toolbar and remembered per extension.
- Old settings files still load.

## Gates
`bun run --bun check`, `bun test src/lib`, `bun run --bun build`, `bun scripts/testing/css-order.ts`.
