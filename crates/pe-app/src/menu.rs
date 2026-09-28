//! The native menu, in the interface language (spec.md 3.5).
//!
//! Tauri's default menu names its items in English whatever the interface
//! says, so the whole menu is built here from one table and rebuilt when the
//! language changes. Two items are ours rather than the platform's: Quit,
//! which has to go through the unsaved-changes guard (spec.md 3.3), and
//! PolarEffects Help, which opens the help window.

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem, Submenu, WINDOW_SUBMENU_ID,
};

/// The id of our Quit item.
pub const QUIT_ID: &str = "pe-quit";
/// The id of the Help window item.
pub const HELP_ID: &str = "pe-help";

/// Every label the menu shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// "About PolarEffects".
    About,
    /// The macOS Services submenu.
    Services,
    /// "Hide PolarEffects".
    Hide,
    /// "Hide Others".
    HideOthers,
    /// "Show All".
    ShowAll,
    /// "Quit PolarEffects".
    Quit,
    /// The File menu (Windows and Linux, where it holds Quit).
    File,
    /// The Edit menu.
    Edit,
    /// "Undo".
    Undo,
    /// "Redo".
    Redo,
    /// "Cut".
    Cut,
    /// "Copy".
    Copy,
    /// "Paste".
    Paste,
    /// "Select All".
    SelectAll,
    /// The View menu.
    View,
    /// "Enter Full Screen".
    Fullscreen,
    /// The Window menu.
    Window,
    /// "Minimize".
    Minimize,
    /// "Zoom" (maximise).
    Maximize,
    /// The Help menu.
    Help,
    /// "PolarEffects Help".
    AppHelp,
    /// The ORC catalogue's provenance in About (spec.md 5.1), a template
    /// filled by [`about_orc`].
    OrcCatalogue,
}

/// Every label, for the test that each has a translation.
pub const ALL: &[Label] = &[
    Label::About,
    Label::Services,
    Label::Hide,
    Label::HideOthers,
    Label::ShowAll,
    Label::Quit,
    Label::File,
    Label::Edit,
    Label::Undo,
    Label::Redo,
    Label::Cut,
    Label::Copy,
    Label::Paste,
    Label::SelectAll,
    Label::View,
    Label::Fullscreen,
    Label::Window,
    Label::Minimize,
    Label::Maximize,
    Label::Help,
    Label::AppHelp,
    Label::OrcCatalogue,
];

/// A menu label in `language` (one of `settings::LANGUAGES`), English for
/// anything else. The wording follows each platform's own menus in that
/// language, so ours reads like every other application's.
pub fn text(language: &str, label: Label) -> &'static str {
    use Label as L;
    match (language, label) {
        ("fr", L::About) => "À propos de PolarEffects",
        ("fr", L::Services) => "Services",
        ("fr", L::Hide) => "Masquer PolarEffects",
        ("fr", L::HideOthers) => "Masquer les autres",
        ("fr", L::ShowAll) => "Tout afficher",
        ("fr", L::Quit) => "Quitter PolarEffects",
        ("fr", L::File) => "Fichier",
        ("fr", L::Edit) => "Édition",
        ("fr", L::Undo) => "Annuler",
        ("fr", L::Redo) => "Rétablir",
        ("fr", L::Cut) => "Couper",
        ("fr", L::Copy) => "Copier",
        ("fr", L::Paste) => "Coller",
        ("fr", L::SelectAll) => "Tout sélectionner",
        ("fr", L::View) => "Présentation",
        ("fr", L::Fullscreen) => "Passer en plein écran",
        ("fr", L::Window) => "Fenêtre",
        ("fr", L::Minimize) => "Réduire",
        ("fr", L::Maximize) => "Agrandir",
        ("fr", L::Help) => "Aide",
        ("fr", L::AppHelp) => "Aide de PolarEffects",
        ("fr", L::OrcCatalogue) => {
            "Catalogue ORC : {records} certificats de jieter/orc-data, commit {commit} du {date}, construit le {built}."
        }

        ("de", L::About) => "Über PolarEffects",
        ("de", L::Services) => "Dienste",
        ("de", L::Hide) => "PolarEffects ausblenden",
        ("de", L::HideOthers) => "Andere ausblenden",
        ("de", L::ShowAll) => "Alle einblenden",
        ("de", L::Quit) => "PolarEffects beenden",
        ("de", L::File) => "Datei",
        ("de", L::Edit) => "Bearbeiten",
        ("de", L::Undo) => "Widerrufen",
        ("de", L::Redo) => "Wiederholen",
        ("de", L::Cut) => "Ausschneiden",
        ("de", L::Copy) => "Kopieren",
        ("de", L::Paste) => "Einfügen",
        ("de", L::SelectAll) => "Alles auswählen",
        ("de", L::View) => "Darstellung",
        ("de", L::Fullscreen) => "Vollbildmodus aktivieren",
        ("de", L::Window) => "Fenster",
        ("de", L::Minimize) => "Minimieren",
        ("de", L::Maximize) => "Zoomen",
        ("de", L::Help) => "Hilfe",
        ("de", L::AppHelp) => "PolarEffects-Hilfe",
        ("de", L::OrcCatalogue) => {
            "ORC-Katalog: {records} Messbriefe aus jieter/orc-data, Commit {commit} vom {date}, erstellt am {built}."
        }

        (_, L::About) => "About PolarEffects",
        (_, L::Services) => "Services",
        (_, L::Hide) => "Hide PolarEffects",
        (_, L::HideOthers) => "Hide Others",
        (_, L::ShowAll) => "Show All",
        (_, L::Quit) => "Quit PolarEffects",
        (_, L::File) => "File",
        (_, L::Edit) => "Edit",
        (_, L::Undo) => "Undo",
        (_, L::Redo) => "Redo",
        (_, L::Cut) => "Cut",
        (_, L::Copy) => "Copy",
        (_, L::Paste) => "Paste",
        (_, L::SelectAll) => "Select All",
        (_, L::View) => "View",
        (_, L::Fullscreen) => "Enter Full Screen",
        (_, L::Window) => "Window",
        (_, L::Minimize) => "Minimize",
        (_, L::Maximize) => "Zoom",
        (_, L::Help) => "Help",
        (_, L::AppHelp) => "PolarEffects Help",
        (_, L::OrcCatalogue) => {
            "ORC catalogue: {records} certificates from jieter/orc-data, commit {commit} of {date}, built {built}."
        }
    }
}

/// The ORC catalogue's provenance for About, in `language`: how many
/// certificates, the orc-data commit and its date, and the build date. Reads
/// only the catalogue's header. `None` if the embedded catalogue is damaged.
pub fn about_orc(language: &str) -> Option<String> {
    let provenance = pe_orc::provenance().ok()?;
    let commit: String = provenance.commit.chars().take(10).collect();
    Some(
        text(language, Label::OrcCatalogue)
            .replace("{records}", &provenance.records.to_string())
            .replace("{commit}", &commit)
            .replace("{date}", &provenance.commit_date)
            .replace("{built}", &provenance.build_date),
    )
}

/// Builds the menu in `language`.
pub fn build<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    language: &str,
) -> tauri::Result<Menu<R>> {
    let l = |label| Some(text(language, label));
    let info = app.package_info();
    // macOS shows `credits` in its About panel, the others `comments`.
    let orc = about_orc(language);
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        comments: orc.clone(),
        credits: orc,
        ..AboutMetadata::default()
    };
    let quit = MenuItem::with_id(
        app,
        QUIT_ID,
        text(language, Label::Quit),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let help = MenuItem::with_id(
        app,
        HELP_ID,
        text(language, Label::AppHelp),
        true,
        Some("F1"),
    )?;

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        app,
        info.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, l(Label::About), Some(about.clone()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, l(Label::Services))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, l(Label::Hide))?,
            &PredefinedMenuItem::hide_others(app, l(Label::HideOthers))?,
            &PredefinedMenuItem::show_all(app, l(Label::ShowAll))?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // No Close Window item: its fixed Cmd/Ctrl-W accelerator would take the
    // chord spec.md 3.2 gives to closing the project. The window's own close
    // button still closes it, through the same guard as Quit.
    #[cfg(not(target_os = "macos"))]
    let file_menu = Submenu::with_items(app, text(language, Label::File), true, &[&quit])?;

    let edit_menu = Submenu::with_items(
        app,
        text(language, Label::Edit),
        true,
        &[
            &PredefinedMenuItem::undo(app, l(Label::Undo))?,
            &PredefinedMenuItem::redo(app, l(Label::Redo))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, l(Label::Cut))?,
            &PredefinedMenuItem::copy(app, l(Label::Copy))?,
            &PredefinedMenuItem::paste(app, l(Label::Paste))?,
            &PredefinedMenuItem::select_all(app, l(Label::SelectAll))?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let view_menu = Submenu::with_items(
        app,
        text(language, Label::View),
        true,
        &[&PredefinedMenuItem::fullscreen(app, l(Label::Fullscreen))?],
    )?;

    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        text(language, Label::Window),
        true,
        &[
            &PredefinedMenuItem::minimize(app, l(Label::Minimize))?,
            &PredefinedMenuItem::maximize(app, l(Label::Maximize))?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        text(language, Label::Help),
        true,
        &[
            &help,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app)?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, l(Label::About), Some(about))?,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let _ = about;

    Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &app_menu,
            #[cfg(not(target_os = "macos"))]
            &file_menu,
            &edit_menu,
            #[cfg(target_os = "macos")]
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}

/// Builds the menu in `language` and makes it the application's. A failure
/// leaves the previous menu, which is still a working menu.
pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>, language: &str) {
    if let Ok(menu) = build(app, language) {
        let _ = app.set_menu(menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LANGUAGES;

    #[test]
    fn every_label_exists_in_every_language_and_names_the_app_where_english_does() {
        for &language in LANGUAGES {
            for &label in ALL {
                let english = text("en", label);
                let translated = text(language, label);
                assert!(!translated.trim().is_empty(), "{language} {label:?}");
                assert_eq!(
                    english.contains("PolarEffects"),
                    translated.contains("PolarEffects"),
                    "{language} {label:?}"
                );
            }
        }
    }

    /// The menu really changes language: apart from words that are the same
    /// in both (Services), every French and German label differs from English.
    #[test]
    fn switching_language_changes_the_labels() {
        for language in ["fr", "de"] {
            let same: Vec<Label> = ALL
                .iter()
                .copied()
                .filter(|&label| text(language, label) == text("en", label))
                .collect();
            let allowed: &[Label] = if language == "fr" {
                &[Label::Services]
            } else {
                &[]
            };
            assert_eq!(same, allowed, "{language}");
        }
        assert_eq!(text("tlh", Label::Quit), "Quit PolarEffects");
    }

    /// About names the catalogue's source commit and dates (spec.md 5.1), in
    /// every language, with every placeholder filled.
    #[test]
    fn about_names_the_orc_catalogue_provenance() {
        let provenance = pe_orc::provenance().unwrap();
        for &language in LANGUAGES {
            let about = about_orc(language).unwrap();
            assert!(about.contains(&provenance.commit[..10]), "{about}");
            assert!(about.contains(&provenance.commit_date), "{about}");
            assert!(about.contains(&provenance.build_date), "{about}");
            assert!(about.contains(&provenance.records.to_string()), "{about}");
            assert!(!about.contains('{'), "{about}");
        }
    }
}
