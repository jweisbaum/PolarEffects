import type { HelpTopic } from "../topics";

const topics: HelpTopic[] = [
  { id: "workspace", group: "Espace de travail", title: "La fenêtre du projet",
    paragraphs: [
      "PolarEffects construit la polaire d’un bateau à partir de certificats ORC, de fichiers de polaires et de traces de course, puis l’exporte pour les logiciels de routage.",
      "La barre de titre contient le menu Projet, le nom du projet, le sélecteur de vue, la recherche et les Réglages. La navigation de gauche rassemble les sources, la scène centrale affiche la carte, la polaire 3D ou la comparaison, et le panneau de droite liste les sources au-dessus d’un diagramme polaire 2D. La barre d’état, en bas, affiche les conseils, les erreurs et les travaux en cours.",
    ],
    parameters: [
      ["Masquer ou afficher la navigation (◀)", "Replie toute la navigation de gauche pour laisser plus de place à la scène. Chacune de ses sections se replie aussi séparément. Ce qui est replié est mémorisé pour vous, pas dans le projet."],
      ["Vue : Carte / 3D / Comparer", "Choisit ce qu’affiche le centre. La carte est la vue par défaut."],
      ["Masquer ou afficher le panneau de droite (▶)", "Replie la liste des sources et le diagramme polaire."],
      ["Barre d’état", "Affiche le dernier conseil ou la dernière erreur, et une ligne par tâche en cours."],
      ["Annuler / Rétablir", "Cmd+Z et Cmd+Maj+Z (Ctrl sous Windows et Linux) annulent ou rétablissent la dernière modification du projet."],
    ],
    related: ["projects", "search", "map", "sources"] },
  { id: "projects", group: "Espace de travail", title: "Projets, enregistrement et récupération",
    paragraphs: [
      "Un projet est une tentative de construire une polaire pour un bateau. Il est enregistré dans un fichier .wpsproj qui conserve chaque source telle qu’elle a été importée, vos modifications étant stockées à côté.",
      "Lorsqu’une action fermerait un projet contenant des modifications non enregistrées — Nouveau, Ouvrir, Ouvrir un projet récent, Fermer ou quitter —, PolarEffects vous le demande d’abord : Enregistrer, Ne pas enregistrer ou Annuler. Annuler, la touche Échap et un clic en dehors de la question laissent le projet ouvert tel quel. Enregistrer un projet jamais enregistré demande où le mettre ; annuler ce choix annule aussi l’action.",
    ],
    parameters: [
      ["Nouveau… (Cmd+N)", "Crée un projet. Le nom est obligatoire ; le nom du bateau et les notes sont facultatifs."],
      ["Ouvrir… (Cmd+O)", "Ouvre un fichier .wpsproj."],
      ["Ouvrir un projet récent", "Liste les dix projets les plus récents, du plus récent au plus ancien."],
      ["Enregistrer (Cmd+S) / Enregistrer sous… (Cmd+Maj+S)", "Écrit le projet dans son fichier, ou dans un nouveau."],
      ["Fermer (Cmd+W)", "Ferme le projet et revient à l’écran d’accueil."],
      ["Nom du projet", "Cliquez sur le nom dans la barre de titre pour renommer le projet. Un point après le nom signale des modifications non enregistrées."],
      ["Projets récents", "Sur l’écran d’accueil. Un fichier déplacé ou supprimé apparaît grisé avec la mention Introuvable ; cliquez dessus pour le retirer de la liste. Effacer oublie toute la liste sans supprimer aucun projet."],
      ["Travail récupéré", "Si PolarEffects ne s’est pas fermé correctement, l’écran d’accueil propose le travail non enregistré qu’il avait conservé. La récupération l’ouvre comme le projet d’origine, toujours non enregistré."],
    ],
    related: ["settings", "workspace"] },
  { id: "orc", group: "Sources", title: "Polaires ORC",
    paragraphs: [
      "La section Polaires ORC de la navigation de gauche permettra de rechercher dans le catalogue intégré des certificats ORC par nom de bateau, numéro de voile ou type, et d’ajouter ceux choisis au projet comme sources. Le catalogue fait partie de l’application et n’est jamais téléchargé.",
    ],
    parameters: [
      ["Polaires ORC (section)", "Cliquez sur le titre pour replier ou déplier la section."],
    ],
    related: ["sources", "polar-files", "tracks"] },
  { id: "polar-files", group: "Sources", title: "Fichiers de polaires",
    paragraphs: [
      "La section Fichiers de polaires permettra d’importer des polaires existantes au format Expedition ou Adrena. Une polaire importée est conservée telle qu’elle a été lue ; vos modifications sont stockées à côté.",
    ],
    parameters: [
      ["Fichiers de polaires (section)", "Cliquez sur le titre pour replier ou déplier la section."],
    ],
    related: ["sources", "orc", "tracks"] },
  { id: "tracks", group: "Sources", title: "Traces",
    paragraphs: [
      "La section Traces permettra d’importer les traces de course du bateau depuis YellowBrick, Geovoile, Blue Water Tracks ou des fichiers GeoJSON et CSV. Chaque position est associée au vent, aux vagues et au courant du moment, et la trace devient un segment de polaire que l’on peut filtrer et fusionner.",
    ],
    parameters: [
      ["Traces (section)", "Cliquez sur le titre pour replier ou déplier la section."],
    ],
    related: ["sources", "map", "orc"] },
  { id: "sources", group: "Sources", title: "Liste des sources et diagramme polaire",
    paragraphs: [
      "Le panneau de droite liste toutes les sources du projet — polaires ORC, fichiers de polaires et traces —, chacune avec sa couleur, un interrupteur pour l’afficher ou la masquer et un poids de fusion. Une source masquée est exclue de la fusion et de tous les graphiques.",
      "Sous la liste, le diagramme polaire 2D trace la vitesse du bateau en fonction de l’angle du vent réel pour une vitesse de vent réel, avec les points des traces.",
    ],
    parameters: [
      ["Sources (section)", "Cliquez sur le titre pour replier ou déplier la liste."],
      ["Diagramme polaire (section)", "Cliquez sur le titre pour replier ou déplier le diagramme."],
    ],
    related: ["workspace", "polar-3d", "compare"] },
  { id: "map", group: "Vues", title: "La carte du monde",
    paragraphs: [
      "La carte est la vue par défaut. Elle dessine les terres et les côtes du monde à partir de données intégrées à l’application ; aucune tuile de carte n’est jamais téléchargée. Les traces y seront dessinées dans la couleur de leur source.",
    ],
    parameters: [
      ["Projection : Équirectangulaire / Orthographique", "Équirectangulaire dessine la longitude et la latitude comme une grille plane. Orthographique dessine un globe vu de l’espace. Le choix est mémorisé pour vous."],
      ["Glisser", "Déplace la carte plane, ou fait tourner le globe."],
      ["Molette ou pincement", "Zoome et dézoome autour du pointeur."],
      ["Voir le monde entier", "Affiche de nouveau le monde entier."],
    ],
    related: ["workspace", "tracks"] },
  { id: "polar-3d", group: "Vues", title: "La polaire 3D",
    paragraphs: [
      "La vue 3D montrera une polaire comme une surface de vitesse du bateau selon l’angle et la vitesse du vent réel, avec les points des traces, et permettra d’exclure des points et de modifier une source à la fois.",
    ],
    related: ["compare", "sources"] },
  { id: "compare", group: "Vues", title: "Comparer",
    paragraphs: [
      "La vue Comparer montrera la différence entre deux polaires — deux sources, ou une source et la fusion —, case par case.",
    ],
    related: ["polar-3d", "sources"] },
  { id: "settings", group: "Réglages et aide", title: "Réglages",
    paragraphs: [
      "Les réglages s’appliquent à toute l’application et à tous les projets, et ne sont jamais stockés dans un projet. Ouvrez-les avec la roue dentée de la barre de titre, le bouton Réglages de l’écran d’accueil ou Cmd+, (Ctrl+, sous Windows et Linux).",
    ],
    parameters: [
      ["Langue", "Anglais, français ou allemand. Tout change aussitôt, y compris la barre des menus. Aussi sur l’écran d’accueil."],
      ["Thème", "Les couleurs de l’application. Port est le thème par défaut."],
      ["Vitesse / Hauteur des vagues / Distance", "Les unités d’affichage des valeurs. Les valeurs stockées ne changent pas."],
      ["Enregistrement automatique", "Garder une copie de récupération du travail non enregistré (par défaut), enregistrer dans le fichier du projet lui-même, ou tout laisser jusqu’à ce que vous enregistriez."],
      ["Cache de blocs", "L’emplacement des données de vent, de vagues et de courant téléchargées, et la taille maximale de ce dossier (20 GB par défaut). Vider le cache le vide ; rien n’est perdu, car chaque valeur utilisée par un projet est enregistrée dans le projet."],
      ["Requêtes simultanées / Délai d’attente", "Combien de téléchargements ont lieu en même temps (8 par défaut) et combien de temps un téléchargement peut durer avant d’être abandonné."],
    ],
    related: ["projects", "search"] },
  { id: "search", group: "Réglages et aide", title: "Recherche et aide",
    paragraphs: [
      "La zone de recherche de la barre de titre trouve n’importe quelle commande par son nom ou par les mots qui la désignent, dans la langue affichée, avec ou sans accents. Les résultats apparaissent pendant la frappe. Choisissez-en un et PolarEffects ouvre ce qui la cache — un panneau, une section, une vue, un menu ou les Réglages — puis l’entoure un instant en orange.",
      "Les pages d’aide sont listées sous les commandes dans les résultats. Cette aide s’ouvre avec F1, le bouton ? ou le menu Aide, et a sa propre recherche.",
    ],
    parameters: [
      ["Recherche (Cmd+F / Ctrl+F)", "Place le curseur dans la zone de recherche depuis n’importe où."],
      ["? (F1)", "Ouvre cette aide."],
      ["Flèches et Entrée", "Choisissent un résultat sans la souris. Échap ferme la liste."],
    ],
    related: ["workspace", "settings"] },
];

export default topics;
