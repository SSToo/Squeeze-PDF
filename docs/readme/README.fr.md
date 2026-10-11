# Squeeze PDF

[English](README.en.md) · [简体中文](../../README.md) · **Français** · [Español](README.es.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Português](README.pt.md)

Compresseur PDF open source et entièrement hors ligne pour Windows et macOS, actuellement au stade MVP. L’interface utilise Tauri 2 et React. Rust gère les stratégies de compression, les images, la reconstruction des pages et la protection des fichiers. La lecture et l’écriture des PDF reposent sur lopdf ; le rendu utilise PDF.js localement sur le bureau et hayro par défaut en ligne de commande. MozJPEG est lié statiquement via Rust. Aucun qpdf, Ghostscript ou Python n’est nécessaire à l’exécution.

Téléchargez les installateurs Windows x64, macOS Apple silicon et macOS Intel dans [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases). Un tag `v` correspondant à la version de l’application publie automatiquement les installateurs et les sommes de contrôle après la réussite des trois builds. Consultez le [guide de publication](../RELEASING.md) (chinois/anglais).

À partir de 0.1.25, les versions distribuées recherchent les mises à jour stables sur GitHub environ cinq secondes après le lancement, puis toutes les 24 heures. L’icône de mise à jour permet une vérification manuelle. Vous choisissez l’installation et le redémarrage, après la fin des tâches en cours. Une vérification hors ligne n’interrompt pas la compression. Seules les mises à jour utilisent le réseau ; les PDF ne sont jamais envoyés. Les versions 0.1.24 et antérieures nécessitent d’abord une mise à niveau manuelle.

## Licence

Le code propre au projet est sous [Apache-2.0](../../LICENSE), avec les mentions dans [NOTICE](../../NOTICE). L’utilisation commerciale et les produits dérivés propriétaires sont autorisés, sous réserve des obligations de licence, de conservation des mentions et d’identification des modifications. Le texte de la licence définit l’autorisation relative aux brevets et ses conditions.

Les bibliothèques, polices et ressources tierces conservent leurs licences. Les distributions incluent `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, les mentions tierces et les sources requises. Voir [Contribuer](../../CONTRIBUTING.md) et le [dossier de préparation open source](../OPEN-SOURCE.md).

## Deux modes de sortie

| Mode                           | Résultat                                                                                     | Usage                                                                            |
| ------------------------------ | -------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Conserver le contenu d’origine | Conserve textes, polices et vecteurs ; optimise tracés, structure et images prises en charge | Rapports, contrats, plans, texte sélectionnable et agrandissement                |
| Pages en images                | Chaque page devient une image distincte, puis un PDF est reconstruit                         | Pages uniformes lorsque texte sélectionnable et vecteurs ne sont pas nécessaires |

Le mode et le niveau de qualité se choisissent séparément :

| Niveau                   | Contenu d’origine                                                                      | Pages en images                               |
| ------------------------ | -------------------------------------------------------------------------------------- | --------------------------------------------- |
| Fidélité / Haute qualité | Résolution d’origine, JPEG 95, optimisation de précision et réutilisation des contours | 300 DPI, pixels encodés sans perte avec Flate |
| Équilibré                | Optimisation des tracés, JPEG 85, jusqu’à 200 DPI                                      | Largeur 1400 px, qualité JPEG 72              |
| Plus léger               | Optimisation des tracés, JPEG 50, jusqu’à 100 DPI                                      | Largeur 1000 px, qualité JPEG 60              |

Fidélité conserve les dimensions des images mais peut modifier leurs pixels : **ce n’est pas un mode strictement sans perte**. L’arrondi des coordonnées vectorielles limite l’erreur à environ 0,0005 point par axe pour les transformations connues. Le positionnement du texte et les couleurs ne sont pas arrondis ; les contours très fins peuvent toutefois changer. Les contours admissibles, fermés et remplis d’une couleur unie, sont réutilisés comme Forms vectoriels.

Haute qualité encode sans perte les pixels déjà rendus : textes et vecteurs sont rasterisés. Le mode image supprime texte sélectionnable, tracés vectoriels, liens et signets. Sur le bureau, formulaires et annotations visibles pouvant être rendus sont intégrés aux images ; les champs masqués et le texte des fenêtres d’annotation ne sont pas conservés. Les données des pièces jointes intégrées sont conservées.

Le mode de conservation n’exporte qu’un PDF validé plus petit que l’original. Les candidats sont comparés comme fichiers complets, avec retour à la solution précédente en cas d’augmentation. Le mode image respecte le format choisi même si le fichier grossit, et affiche cette augmentation. Les originaux sont en lecture seule, les sorties existantes ne sont jamais écrasées et une modification de l’entrée bloque la validation de la sortie.

Le moteur compare JPEG, Flate exact, palettes Indexed, encodage 1 bit et CCITT G4 selon les entrées prises en charge. Il optimise aussi coefficients JPEG, partage des ressources, certains sous-ensembles de polices CID et disposition du fichier. Le rendu bureau peut comparer le rendu direct à un rendu 2× réduit, dans les budgets et seuils de qualité définis. Voir les [limites d’implémentation et de validation](../VALIDATION.md).

## Fonctions de bureau

- Glisser ou sélectionner plusieurs PDF, dédupliquer et traiter dans l’ordre.
- Deux modes, trois niveaux, étapes en cours et nombre de fichiers terminés.
- Tailles avant/après, gain ou augmentation, détails et erreurs directement dans la liste.
- Aperçu côte à côte, navigation et zoom ; toutes les ressources d’aperçu sont locales.
- Choix du dossier, numérotation automatique des noms, annulation, nouvelle tentative et ouverture du résultat ou du dossier.
- Mémorisation du mode, du niveau, du dossier et du thème ; la file ne dure que pendant la session.
- Menu de langue à droite du thème : anglais, chinois simplifié, français, espagnol, coréen, japonais, allemand et portugais. Au premier lancement, langue système prise en charge ou anglais ; choix suivants mémorisés.
- Interface, étapes, erreurs et aperçu changent de langue sans interrompre le travail. Noms, chemins et erreurs techniques inconnues restent inchangés.
- Logo transparent, accents rouges et thème sombre anthracite.

## Développement

Prérequis : Node.js 22.13+, Rust 1.92.0 fixé par `rust-toolchain.toml` et compilateur C de la plateforme. macOS nécessite Xcode Command Line Tools ; Windows, MSVC C++ Build Tools et WebView2. MozJPEG est compilé dans le worker ; SIMD en assembleur désactivé, sans besoin de NASM ni CMake.

Exécuter depuis la racine du dépôt :

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` ne lance que l’interface ; les fonctions de fichiers natives exigent `tauri dev`. Les scripts préparent le worker de l’architecture courante, PDF.js et les licences. `private: true` évite une publication npm accidentelle et ne modifie pas la licence open source.

## Vérifications et paquets

```sh
cargo test --workspace --locked
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# Application macOS
npm run tauri -- build --bundles app
# Application macOS et DMG simple
npm run package:mac
# Installateur EXE ; à exécuter sous Windows
npm run tauri -- build --bundles nsis
```

Les paquets sont dans `target/release/bundle/`, pour l’architecture hôte. Avec `--target`, définir aussi `CARGO_BUILD_TARGET` pour aligner le worker. L’installateur Windows inclut l’installateur hors ligne de WebView2.

La CI cible Windows x64, macOS arm64 et Intel. Les validations locales ont été faites sur macOS arm64 ; les essais d’installation Windows/Intel restent à réaliser. Les paquets macOS utilisent une signature ad-hoc locale. Signature Windows de production, Developer ID macOS et notarisation ne sont pas configurés. Pour publier, définir `APPLE_SIGNING_IDENTITY` et suivre la [documentation Tauri](https://v2.tauri.app/distribute/sign/macos/).

## CLI et moteur

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode` : `preserve` par défaut ou `raster`. Niveaux : `lossless`, `fidelity`, `balanced`, `small` ; `--quality 85 --dpi 200` s’applique aux niveaux compatibles. `--compact-paths` et `--reuse-paths` activent la précision des tracés et la réutilisation des contours ; désactivés par défaut dans la CLI, activés pour les trois niveaux de conservation sur le bureau.

`lossless` ne requantifie ni ne réduit les images et ignore ces deux options vectorielles ; les encodages exacts, l’optimisation JPEG des coefficients et le partage équivalent restent possibles. `fidelity` conserve les dimensions, utilise JPEG 95 et n’accepte pas de DPI cible. Haute qualité utilise l’encodage sans perte des pixels. Ctrl+C demande l’annulation. Sur le bureau, elle termine le rendu ou le worker séparé. Le rendu des pages et l’encodage worker sont chacun limités à 120 secondes.

API : `squeeze_core::inspect_file`, `squeeze_core::audit_file`, `squeeze_core::compress`. `squeeze audit input.pdf` fournit une répartition de taille en lecture seule. Le JSON décrit tailles, étapes, candidats d’images, durées, cache et choix d’écriture. Les coûts de flux/candidats ne sont pas la taille du PDF complet et ne signifient pas que ces octets sont superflus.

## Prise en charge et limites

- Pas de plafond fixe de taille PDF ; plus de 256 MiB possible selon la mémoire disponible. Flux décompressé limité à 64 MiB, image à 24 millions de pixels ; traitement séquentiel.
- Images RGB/Gray, certains ICCBased, Indexed, G4 et composants couleur bruts pris en charge. Encodages ou mappages inconnus conservés prudemment ; certains JPEG CMYK/YCCK permettent une optimisation des coefficients sans réencodage des pixels. Dimensions et sémantique couleur/Decode/masques sont protégées selon le cas ; tous les codecs ne sont pas pris en charge.
- Échelle inconnue ou analyse incomplète : réduction désactivée pour les images concernées. Fidélité garde toujours les dimensions d’origine.
- Conservation des formulaires, annotations et pièces jointes ; protection des apparences et images partagées. Les images ordinaires indépendantes restent optimisables. Les formulaires/annotations d’apparence désactivent prudemment les deux optimisations vectorielles avec perte. PDF chiffrés, signés, PDF/A, XFA et portfolios ignorés.
- Le rendu bureau accepte formulaires/annotations rendables, liens ordinaires, conteneurs de formulaire vides et pièces jointes. Apparence manquante ou annotation endommagée : erreur de page. Le rendu hayro CLI refuse les formulaires/annotations à dessiner et les flux `BrotliDecode` ; utiliser PDF.js sur le bureau.
- Rotation, zone visible et dimensions physiques traitées. Conversion refusée si une police non standard n’est pas intégrée. Vérifier visuellement les pages complexes ; hayro évolue et ne garantit pas toute la compatibilité des PDF commerciaux.
- Le parseur peut réparer les références croisées ; tous les PDF malformés ne sont pas strictement refusés. Les limites existantes ne constituent pas un budget mémoire global.
- L’annulation normale nettoie les répertoires temporaires du worker arrêté. Pas de récupération automatique au démarrage après arrêt forcé ou coupure électrique.
- Pas d’OCR, d’édition PDF, de taille finale garantie ni de système d’activation commerciale.

## Mesures et dossiers techniques

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

Le grand échantillon ajoute un PDF de 24 pages d’environ 138 MB pour les essais d’annulation et de gros fichiers. Les taux synthétiques ne prédisent pas les documents réels. Poppler, pypdf, Pillow et NumPy servent aux vérifications indépendantes de développement, sans dépendance d’exécution.

Dossiers détaillés actuellement en chinois : [validation](../VALIDATION.md), [plan](../../PLAN.md). Les essais Jpegli et Brotli ne sont ni des dépendances produit ni des options de sortie.

## Matériaux tiers

Les fichiers de verrouillage fixent les dépendances. `npm run notices` rassemble licences, NOTICE hayro, mentions des polices/WASM PDF.js, MIT QuickJS/Mozilla, sources MPL non modifiées et sources Liberation 1.07.4, avec les sources TTX éditables des polices réellement distribuées. Tout est placé dans `third-party/` ; l’inventaire inclut prudemment les dépendances de compilation. Voir les [instructions de reconstruction](../../licenses/SOURCE-CODE.md) et le [dossier de licences](../LICENSING-0.1.22.md).

`npm run test:licenses` contrôle textes, empreintes des ressources/sources et intégrité MPL dans la CI Windows/macOS. Toute mise à jour PDF.js/ressources exige une nouvelle revue. `scripts/make-font-sources.py` utilise FontTools 4.66.1 uniquement en développement. Liberation conserve sa GPL avec exceptions pour les polices ; les autres composants gardent leurs licences.

PDF : [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/) ; rendu : [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) ou [hayro](https://github.com/LaurenzV/hayro) ; décodage : image ; JPEG : MozJPEG ; G4 : `fax 0.3.0` sous MIT ; sous-ensembles CID TrueType limités : `subsetter 0.2.6` sous MIT/Apache-2.0. Les polices de test ne sont pas distribuées. Le code propre au projet est Apache-2.0 depuis 0.1.23 ; les matériaux tiers ne sont pas relicenciés.
