import type { HelpTopic } from "../topics";

const topics: HelpTopic[] = [
  { id: "workspace", group: "Arbeitsbereich", title: "Das Projektfenster",
    paragraphs: [
      "PolarEffects erstellt die Polare eines Bootes aus ORC-Messbriefen, Polardateien und Regattatracks und exportiert sie für Routing-Software.",
      "Die Titelleiste enthält das Projektmenü, den Projektnamen, die Ansichtsauswahl, die Suche und die Einstellungen. Die linke Navigation sammelt die Quellen, die Bühne in der Mitte zeigt die Karte, die 3D-Polare oder den Vergleich, und die rechte Seitenleiste listet die Quellen über einem 2D-Polardiagramm. Die Statusleiste unten zeigt Hinweise, Fehler und laufende Arbeiten.",
    ],
    parameters: [
      ["Navigation aus- oder einblenden (◀)", "Klappt die ganze linke Navigation ein, damit die Bühne mehr Platz hat. Jeder ihrer Abschnitte lässt sich auch einzeln einklappen. Was eingeklappt ist, wird für Sie gespeichert, nicht im Projekt."],
      ["Ansicht: Karte / 3D / Vergleich", "Wählt, was die Mitte zeigt. Die Karte ist die Standardansicht."],
      ["Rechte Seitenleiste aus- oder einblenden (▶)", "Klappt die Quellenliste und das Polardiagramm ein."],
      ["Statusleiste", "Zeigt den letzten Hinweis oder Fehler und eine Zeile für jede laufende Aufgabe."],
      ["Widerrufen / Wiederholen", "Cmd+Z und Cmd+Umschalt+Z (Strg unter Windows und Linux) machen die letzte Änderung am Projekt rückgängig oder wiederholen sie."],
    ],
    related: ["projects", "search", "map", "sources"] },
  { id: "projects", group: "Arbeitsbereich", title: "Projekte, Speichern und Wiederherstellen",
    paragraphs: [
      "Ein Projekt ist ein Versuch, eine Polare für ein Boot zu erstellen. Es wird als .wpsproj-Datei gespeichert, die jede Quelle genau so enthält, wie sie importiert wurde; Ihre Änderungen werden daneben gespeichert.",
      "Wenn eine Aktion ein Projekt mit ungespeicherten Änderungen schließen würde — Neu, Öffnen, Zuletzt geöffnet, Schließen oder Beenden —, fragt PolarEffects zuerst: Speichern, Nicht speichern oder Abbrechen. Abbrechen, die Esc-Taste und ein Klick neben die Frage lassen das Projekt unverändert offen. Beim Speichern eines noch nie gespeicherten Projekts wird nach dem Ort gefragt; wer diese Auswahl abbricht, bricht auch die Aktion ab.",
    ],
    parameters: [
      ["Neu… (Cmd+N)", "Erstellt ein Projekt. Der Name ist Pflicht; Bootsname und Notizen sind optional."],
      ["Öffnen… (Cmd+O)", "Öffnet eine .wpsproj-Datei."],
      ["Zuletzt geöffnet", "Listet die zehn zuletzt verwendeten Projekte, das neueste zuerst."],
      ["Speichern (Cmd+S) / Speichern unter… (Cmd+Umschalt+S)", "Schreibt das Projekt in seine Datei oder in eine neue."],
      ["Schließen (Cmd+W)", "Schließt das Projekt und kehrt zum Startbildschirm zurück."],
      ["Projektname", "Klicken Sie auf den Namen in der Titelleiste, um das Projekt umzubenennen. Ein Punkt hinter dem Namen zeigt ungespeicherte Änderungen an."],
      ["Zuletzt verwendete Projekte", "Auf dem Startbildschirm. Eine verschobene oder gelöschte Datei erscheint ausgegraut als Nicht gefunden; ein Klick entfernt sie aus der Liste. Leeren vergisst die ganze Liste, ohne ein Projekt zu löschen."],
      ["Wiederhergestellte Arbeit", "Wurde PolarEffects nicht ordnungsgemäß beendet, bietet der Startbildschirm die aufbewahrte ungespeicherte Arbeit an. Beim Wiederherstellen wird sie als das ursprüngliche Projekt geöffnet, weiterhin ungespeichert."],
    ],
    related: ["settings", "workspace"] },
  { id: "orc", group: "Quellen", title: "ORC-Polaren",
    paragraphs: [
      "Im Abschnitt ORC-Polaren der linken Navigation werden Sie den eingebauten Katalog der ORC-Messbriefe nach Bootsname, Segelnummer oder Typ durchsuchen und die gewählten als Quellen zum Projekt hinzufügen können. Der Katalog ist Teil der Anwendung und wird nie heruntergeladen.",
    ],
    parameters: [
      ["ORC-Polaren (Abschnitt)", "Klicken Sie auf die Überschrift, um den Abschnitt ein- oder auszuklappen."],
    ],
    related: ["sources", "polar-files", "tracks"] },
  { id: "polar-files", group: "Quellen", title: "Polardateien",
    paragraphs: [
      "Im Abschnitt Polardateien werden Sie vorhandene Polaren im Expedition- oder Adrena-Format importieren können. Eine importierte Polare bleibt so erhalten, wie sie gelesen wurde; Ihre Änderungen werden daneben gespeichert.",
    ],
    parameters: [
      ["Polardateien (Abschnitt)", "Klicken Sie auf die Überschrift, um den Abschnitt ein- oder auszuklappen."],
    ],
    related: ["sources", "orc", "tracks"] },
  { id: "tracks", group: "Quellen", title: "Tracks",
    paragraphs: [
      "Im Abschnitt Tracks werden Sie Regattatracks des Bootes aus YellowBrick, Geovoile, Blue Water Tracks oder GeoJSON- und CSV-Dateien importieren können. Jede Position wird mit Wind, Wellen und Strömung des Zeitpunkts verknüpft, und der Track wird zu einem Polarsegment, das gefiltert und eingemischt werden kann.",
    ],
    parameters: [
      ["Tracks (Abschnitt)", "Klicken Sie auf die Überschrift, um den Abschnitt ein- oder auszuklappen."],
    ],
    related: ["sources", "map", "orc"] },
  { id: "sources", group: "Quellen", title: "Quellenliste und Polardiagramm",
    paragraphs: [
      "Die rechte Seitenleiste listet alle Quellen des Projekts — ORC-Polaren, Polardateien und Tracks —, jede mit ihrer Farbe, einem Schalter zum Ein- und Ausblenden und einer Mischgewichtung. Eine ausgeblendete Quelle fließt weder in die Mischung noch in ein Diagramm ein.",
      "Unter der Liste zeigt das 2D-Polardiagramm die Bootsgeschwindigkeit über dem wahren Windwinkel für eine wahre Windgeschwindigkeit, mit den Punkten der Tracks.",
    ],
    parameters: [
      ["Quellen (Abschnitt)", "Klicken Sie auf die Überschrift, um die Liste ein- oder auszuklappen."],
      ["Polardiagramm (Abschnitt)", "Klicken Sie auf die Überschrift, um das Diagramm ein- oder auszuklappen."],
    ],
    related: ["workspace", "polar-3d", "compare"] },
  { id: "map", group: "Ansichten", title: "Die Weltkarte",
    paragraphs: [
      "Die Karte ist die Standardansicht. Sie zeichnet Land und Küsten der Welt aus Daten, die in die Anwendung eingebaut sind; es werden nie Kartenkacheln heruntergeladen. Die Tracks werden darauf in der Farbe ihrer Quelle gezeichnet.",
    ],
    parameters: [
      ["Projektion: Plattkarte / Orthografisch", "Die Plattkarte zeichnet Länge und Breite als ebenes Gitter. Orthografisch zeigt einen Globus wie aus dem All gesehen. Die Wahl wird für Sie gespeichert."],
      ["Ziehen", "Verschiebt die flache Karte oder dreht den Globus."],
      ["Scrollen oder Zusammenziehen", "Zoomt um den Mauszeiger herum hinein und heraus."],
      ["Ganze Welt zeigen", "Zeigt wieder die ganze Welt."],
    ],
    related: ["workspace", "tracks"] },
  { id: "polar-3d", group: "Ansichten", title: "Die 3D-Polare",
    paragraphs: [
      "Die 3D-Ansicht wird eine Polare als Fläche der Bootsgeschwindigkeit über wahrem Windwinkel und wahrer Windgeschwindigkeit zeigen, mit den Punkten der Tracks, und das Ausschließen von Punkten sowie das Bearbeiten jeweils einer Quelle erlauben.",
    ],
    related: ["compare", "sources"] },
  { id: "compare", group: "Ansichten", title: "Vergleich",
    paragraphs: [
      "Die Ansicht Vergleich wird den Unterschied zwischen zwei Polaren zeigen — zwei Quellen oder einer Quelle und der Mischung —, Zelle für Zelle.",
    ],
    related: ["polar-3d", "sources"] },
  { id: "settings", group: "Einstellungen und Hilfe", title: "Einstellungen",
    paragraphs: [
      "Die Einstellungen gelten für die ganze Anwendung und alle Projekte und werden nie in einem Projekt gespeichert. Öffnen Sie sie mit dem Zahnrad in der Titelleiste, der Schaltfläche Einstellungen auf dem Startbildschirm oder Cmd+, (Strg+, unter Windows und Linux).",
    ],
    parameters: [
      ["Sprache", "Englisch, Französisch oder Deutsch. Alles wechselt sofort, auch die Menüleiste. Auch auf dem Startbildschirm."],
      ["Design", "Die Farben der Anwendung. Hafen ist die Voreinstellung."],
      ["Geschwindigkeit / Wellenhöhe / Entfernung", "Die Einheiten, in denen Werte angezeigt werden. Gespeicherte Werte ändern sich nicht."],
      ["Automatisches Speichern", "Eine Wiederherstellungskopie ungespeicherter Arbeit behalten (Voreinstellung), in die Projektdatei selbst speichern oder alles bis zum Speichern lassen."],
      ["Datenblock-Cache", "Wo heruntergeladene Wind-, Wellen- und Strömungsdaten liegen und wie groß dieser Ordner werden darf (standardmäßig 20 GB). Cache leeren leert ihn; nichts geht verloren, denn jeder Wert, den ein Projekt verwendet, ist im Projekt gespeichert."],
      ["Gleichzeitige Anfragen / Zeitlimit", "Wie viele Downloads gleichzeitig laufen (standardmäßig 8) und wie lange einer dauern darf, bevor er aufgegeben wird."],
    ],
    related: ["projects", "search"] },
  { id: "search", group: "Einstellungen und Hilfe", title: "Suche und Hilfe",
    paragraphs: [
      "Das Suchfeld in der Titelleiste findet jedes Bedienelement über seinen Namen oder über Wörter, unter denen man es kennt, in der angezeigten Sprache, mit oder ohne Akzente und Umlaute. Die Ergebnisse erscheinen schon beim Tippen. Wählen Sie eines, und PolarEffects öffnet, was es verdeckt — eine Seitenleiste, einen Abschnitt, eine Ansicht, ein Menü oder die Einstellungen — und umrandet es kurz orange.",
      "Hilfeseiten stehen in den Ergebnissen unter den Bedienelementen. Diese Hilfe öffnet sich mit F1, der Schaltfläche ? oder dem Hilfemenü und hat eine eigene Suche.",
    ],
    parameters: [
      ["Suche (Cmd+F / Strg+F)", "Springt von überall in das Suchfeld."],
      ["? (F1)", "Öffnet diese Hilfe."],
      ["Pfeiltasten und Eingabetaste", "Wählen ein Ergebnis ohne Maus. Esc schließt die Liste."],
    ],
    related: ["workspace", "settings"] },
];

export default topics;
