# Translation glossary

The words the interface and the help reference use for PolarEffects' own
concepts, fixed so every area — and every later feature — says the same
thing. The audience is navigators, routers and performance analysts: prefer
the word a French or German sailor, a class rule or a routing program would
use, not a dictionary's.

Copied in form from VectorEffects' glossary. The translations were written by
the implementer (plan.md M2); **they are marked for review by native-speaking
sailors in M17**.

**Register.** French addresses the reader as *vous*; German as *Sie*. Button
and menu labels are infinitives in French ("Enregistrer") and infinitives or
nouns in German ("Speichern", "Einstellungen").

**Punctuation.** French puts a narrow no-break space (U+202F) before
`: ; ? !` and uses « » with no-break spaces inside; German uses „ “. Keep `…`
where the English has it (it means "opens a dialog"). Keep every
`{placeholder}` exactly as written.

**Never translated:** PolarEffects, ORC, TWA, TWS, BSP, VMG, Expedition,
Adrena, YellowBrick, Geovoile, Blue Water Tracks, GeoJSON, CSV, GRIB, ERA5,
WebGL2, file extensions (.wpsproj), unit symbols (kn, m/s, km/h, m, ft, nm,
km, GB, s, °).

**Menus** follow each platform's own wording in that language (macOS:
"Quitter", "Widerrufen", "Einsetzen" would be macOS German; we use the
cross-platform "Einfügen"). The native menu's labels live in Rust,
`crates/pe-app/src/menu.rs`, and must agree with this table.

| English | Français | Deutsch | Note |
|---|---|---|---|
| polar | polaire (f.) | Polare (f.) | Never "diagramme polaire" for the data itself. |
| polar plot / polar diagram | diagramme polaire | Polardiagramm | The 2D drawing. |
| blend (noun) | fusion | Mischung | The exported result of the enabled sources. |
| blend (verb) | fusionner | einmischen | |
| source | source | Quelle | Anything that contributes to the polar. |
| weight (blend weight) | poids | Gewichtung | |
| overlay | surcouche | Überlagerung | A user change stored beside a source. |
| track | trace | Track | German sailors say "Track"; not "Spur". |
| tracker | balise | Tracker | The position-reporting device and service. |
| race | course | Regatta | |
| sample (a track position) | point | Punkt | Shown as a dot; "échantillon"/"Messpunkt" only in technical text. |
| polar segment | segment de polaire | Polarsegment | |
| polar node (a grid point of a polar) | nœud de polaire | Polarknoten | "nœud de grille" / "Gitterpunkt" in descriptions. |
| exclude / include (from the blend) | exclure / inclure | ausschließen / aufnehmen | |
| surface (3D) | surface | Fläche | |
| polar tower (3D layout) | tour polaire | Polarturm | |
| ORC certificate | certificat ORC | ORC-Messbrief | "Messbrief" is the class-rule word. |
| sister ship | sistership | Schwesterschiff | |
| TWA — true wind angle | angle du vent réel | wahrer Windwinkel | Keep "TWA" as the abbreviation. |
| TWS — true wind speed | vitesse du vent réel | wahre Windgeschwindigkeit | Keep "TWS". |
| BSP — boat speed | vitesse du bateau | Bootsgeschwindigkeit | Keep "BSP". |
| VMG | VMG | VMG | |
| heading | cap | Kurs | |
| tack (side) | amure | Bug | "tribord amure" / "Steuerbordbug". |
| to tack (manoeuvre) | virer de bord | wenden | Noun: virement / Wende. |
| to gybe | empanner | halsen | Noun: empannage / Halse. |
| to bear away | abattre | abfallen | Noun: abattée / Abfallen. |
| to luff up | lofer | anluven | |
| upwind / downwind | au près / au portant | am Wind / vor dem Wind | |
| wind / waves / current | vent / vagues / courant | Wind / Wellen / Strömung | Current is "toward", wind and waves "from". |
| wave height | hauteur des vagues | Wellenhöhe | Significant height: hauteur significative / signifikante Wellenhöhe. |
| reanalysis | réanalyse | Reanalyse | |
| chunk cache | cache de blocs | Datenblock-Cache | Downloaded reanalysis data. |
| knots | nœuds | Knoten | Symbol "kn" in every language. |
| nautical miles | milles nautiques | Seemeilen | Symbol "nm". |
| project | projet | Projekt | |
| start screen | écran d'accueil | Startbildschirm | |
| stage (Map / 3D / Compare) | vue | Ansicht | |
| Map / 3D / Compare | Carte / 3D / Comparer | Karte / 3D / Vergleich | |
| navigation (left panel) | navigation | Navigation(sleiste) | |
| settings | réglages | Einstellungen | |
| theme | thème | Design | Harbour: Port / Hafen. Midnight: Minuit / Mitternacht. |
| autosave | enregistrement automatique | automatisches Speichern | |
| recovered work | travail récupéré | wiederhergestellte Arbeit | |
| unsaved changes | modifications non enregistrées | ungespeicherte Änderungen | |
| Save / Don't save / Cancel | Enregistrer / Ne pas enregistrer / Annuler | Speichern / Nicht speichern / Abbrechen | |
| Open Recent | Ouvrir un projet récent | Zuletzt geöffnet | |
| equirectangular | équirectangulaire | Plattkarte | |
| orthographic | orthographique | orthografisch | |
| feature search | recherche de commandes | Funktionssuche | |
| control (UI) | commande | Bedienelement | |
