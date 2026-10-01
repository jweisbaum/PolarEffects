# Translation review checklist

PolarExplorer ships in English, French and German (spec.md 3.5). The French
and German were written by the implementer, not by native speakers, and no
native-speaking sailor was available when M17 was built. This is the
checklist for the people who review them. One reviewer per language; a
sailor who races or routes in that language is worth more than a
professional translator who does not sail.

Review status of everything below, until a reviewer signs it off:
**machine-drafted, needs a native sailor.**

## Where the words are

| What | File | How it is keyed |
|---|---|---|
| Interface text | `ui/src/i18n/locales/{fr,de}/<area>.ts` | English text → translation, one line each |
| Feature search labels, descriptions, keywords | the same catalogues (the registry in `ui/src/help/features/*.ts` holds the English) | as above |
| Help pages | `ui/src/help/locales/{fr,de}.ts` | whole pages, same order and ids as `ui/src/help/topics.ts` |
| Native menu bar | `crates/pe-app/src/menu.rs`, `fn text` | one arm per label |
| History and job names from Rust | `ui/src/i18n/rust-strings.json` (English), translated in the catalogues | as above |
| Fixed terms | `ui/src/i18n/GLOSSARY.md` | one row per concept |

A key may end in a context, `@@verb` in `"Compare@@verb"`: English never
shows it, and it lets one English word take two translations.

Edit the translation, never the English key: the English is the key, and
changing it orphans every translation. `npm run ui:test` then checks that
nothing is missing or unused, that placeholders survive and that one
English string is translated one way everywhere.

## 1. Glossary first

Read `ui/src/i18n/GLOSSARY.md` before anything else and settle it: every
other string follows it.

- [ ] Every sailing, meteorological and ORC term reads as a sailor in your
  language would say it (TWA, TWS, BSP, VMG, polar, tack, gybe, beat and run
  angles, leeway, current, significant wave height, mean wave direction,
  reanalysis, sail number, builder, designer, division, leg).
- [ ] The rows marked ⚑ are the implementer's least certain choices. Decide
  each; say so in its note.
- [ ] Decide the knot symbol for French: the interface writes `kn` in every
  language (glossary, last row of the sailing table). If French should use
  `nd`, it changes in the catalogues, the help and `SPEED_SYMBOL` in
  `ui/src/polar/view3d.ts` (a code change, raise it).
- [ ] Decide "blend" (fusion / Mischung) and "polar segment" (segment de
  polaire / Polarsegment): these name PolarExplorer' own ideas and appear
  everywhere.
- [ ] Decide the tracker status words (Racing, Finished, Retired, Did not
  start, Did not finish) against your racing rules' translation.

## 2. Interface text

Work through the app with the language switched (Settings → Language, or the
picker on the start screen), area by area, with the catalogue open beside it.

- [ ] Start screen: title, New project dialog, recent and recovered lists.
- [ ] Title bar: Project menu, project name, stage switcher, search box,
  Settings button; the tooltips of each.
- [ ] Left navigation: ORC polars (search, Search by field and every field
  box, results, Add, the confirmation), Polar files (Import…, errors naming
  line and column), Tracks (File…, YellowBrick…, Geovoile…, Blue Water…,
  the track list, every filter and its tooltip, Fetch weather…, the fetch
  dialog, Export reanalysis GRIB… and its dialog).
- [ ] The tracker import dialog: address, Open, the boat table, statuses,
  errors (unknown event, tracker not answering, older tracker).
- [ ] The CSV import dialog: column names, time formats, speed units.
- [ ] Right panel: source list (Blend entry, colour, visibility, weight,
  Edit, Compare), Blend settings dialog, Export dialog and its errors,
  the polar plot and its hover.
- [ ] Map: projection, fit buttons, hover of a track position.
- [ ] 3D: layout, cameras, tools, Show options, colouring, selection panel,
  Exclude / Include, edit mode (tools, table, statistic, Done).
- [ ] Compare: operands, swap, surfaces, % switch, threshold, summary,
  heat map hover.
- [ ] Settings: every row and every option.
- [ ] Status bar: job names, errors, undo and redo messages (Undo a few
  changes and read them).
- [ ] Native menu bar (macOS): every menu and item.

For each string:

- [ ] Says the same thing as the English, no more and no less.
- [ ] Uses the glossary's word for every fixed concept.
- [ ] Register: French *vous*, German *Sie*; buttons are infinitives
  (French) or infinitives or nouns (German).
- [ ] Fits: nothing clipped or wrapped badly in a button, a tab or a
  table header. German runs longest; the M17b screenshots found no
  clipping, but check narrow windows.
- [ ] Punctuation: French narrow no-break space (U+202F) before `: ; ? !`,
  « » with no-break spaces; German „ “. `…` kept where the English has it.
- [ ] Every `{placeholder}` kept exactly (the test enforces this, but check
  the sentence still reads with the values in it).
- [ ] Keyboard names as your keyboard prints them: French *Maj*, *Échap*,
  *Entrée*; German *Umschalt*, *Esc*, *Eingabetaste*. Chords in tooltips
  and menus are built by `chordText` (`ui/src/chords.ts`): ⌘ on a Mac,
  Ctrl / Strg elsewhere, Maj / Umschalt for Shift.

## 3. Feature search

The search box (Cmd/Ctrl+F) finds every control by its translated label and
its keywords (spec.md 3.6).

- [ ] Type 20 words a sailor in your language would try for things the app
  does (for example: *virement*, *empannage*, *courant*, *marée*, *vagues*,
  *polaire*, *exporter*, *routage*, *balise*, *YellowBrick*; *Wende*,
  *Halse*, *Strömung*, *Gezeiten*, *Wellen*, *Polare*, *exportieren*,
  *Routing*, *Tracker*, *Messbrief*). Each should find the control you
  meant within the first few results. Note any that do not.
- [ ] Keywords: add the words people actually use; remove any that mislead.
- [ ] Descriptions (shown under each result) read naturally.

## 4. Help

- [ ] Open Help (F1) and read every page in your language beside the English
  page of the same name. Pages are prose, translated whole: fix anything that
  reads as a translation.
- [ ] The names of controls in the help match the interface exactly (the
  help says "Récupérer la météo…" only if the button says so).
- [ ] Help search finds pages by the words a sailor would use.

## 5. Sign-off

- [ ] Change "machine-drafted, needs a native sailor" to "reviewed by
  <name>, <date>" in `GLOSSARY.md` for the rows you settled.
- [ ] Run `npm run ui:test` and `npm run ux` (the language flow screenshots
  every main area in French and German) and look at the screenshots in the
  run's directory.
- [ ] Note what you changed and why in the commit message.
