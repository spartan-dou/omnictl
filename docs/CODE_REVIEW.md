# Revue de code d'omnictl

Revue réalisée sur la branche `main` au commit `aee208c` (version 0.7.0), par lecture de
l'ensemble des sources (`src/`, `build.rs`, `packaging/`, `linux/`, workflow CI, scripts).

Limites de la revue :

- lecture statique uniquement : aucun matériel ASUS n'était disponible, et `cargo` n'était pas
  installé sur la machine de revue, donc les tests n'ont pas été exécutés ;
- les points qui restent des hypothèses sont marqués **(hypothèse)** ; tout le reste a été
  vérifié dans le code cité.

Échelle de sévérité : critique / élevée / moyenne / faible. Aucun constat critique.

## Résumé : les points les plus importants

1. **[Élevée] Mise à jour intégrée sans preuve d'authenticité** (S1). Les empreintes SHA-256
   viennent de la même release que les binaires : elles protègent d'un téléchargement abîmé,
   pas d'une release malveillante. Les exécutables ne sont pas signés et les actions de la CI
   qui les produisent sont référencées par tag mobile.
2. **[Moyenne] Mise à jour MSI : fichier vérifié puis réécrit à un chemin prévisible de `%TEMP%`
   avant une installation avec élévation** (S2), ce qui ouvre une fenêtre de substitution.
3. **[Moyenne] Import de configuration destructif et non atomique** (S3). Les fichiers existants
   sont supprimés avant l'écriture des nouveaux, et une entrée nommée `[apps]` passe la
   validation puis fait échouer l'écriture : la configuration est perdue.
4. **[Moyenne] L'interface et la ligne de commande / le serveur MCP ne partagent pas leur état**
   (D1). Chacun réécrit `keys.txt` depuis sa propre copie ; l'interface ouverte écrase ou
   repeint par-dessus ce que le MCP vient d'appliquer.
5. **[Moyenne] Sondages permanents dans le fil de travail** (P1, P2) : boucle à 200 Hz qui
   interroge l'état de toutes les touches dès que la réaction à la frappe est active, et rondes
   HID chaque seconde, avec énumération de tous les périphériques HID quand le clavier est absent.

À noter aussi : le cœur du protocole (`proto.rs`, `link.rs`, `mouse.rs`) n'a aucun test (D2), et
un bug exclut la touche F de la résolution par lettre gravée (D4).

## Points positifs constatés

- Aucune dépendance externe (`Cargo.lock` ne contient que le paquet lui-même) : pas de surface
  d'attaque par la chaîne de dépendances, rien à auditer côté crates.
- Aucun secret en dur dans le dépôt ni dans le workflow ; le jeton de release est celui de
  GitHub Actions, avec `permissions: contents: read` par défaut et `write` pour le seul job de
  publication (`.github/workflows/build.yml:9-10`, `:126-127`).
- Serveur MCP sur stdin/stdout uniquement, arguments inconnus refusés (`src/mcp.rs:187-195`),
  tout validé avant le premier envoi au clavier (`src/mcp.rs:378-379`), commande `raw` non
  exposée au MCP.
- Noms de fichiers assainis pour les profils par application et les imports, avec tests
  d'entrées hostiles (`src/store.rs:80-87`, `:220-226`, `:341-350`).
- Analyseur JSON borné en profondeur (`src/json.rs:114`, `:152-154`).
- Architecture GUI saine dans son principe : le fil d'interface ne parle jamais au matériel,
  les commandes sont regroupées (`src/bin/omnictl-gui/app.rs:302-336`).

## 1. Sécurité

### S1. Mise à jour sans authentification de l'éditeur — élevée

- `src/bin/omnictl-gui/update.rs:15` : tout est téléchargé depuis `releases/latest/download/`.
- `src/bin/omnictl-gui/update.rs:79-94` : `SHA256SUMS.txt` vient de la même release que le
  binaire qu'il sert à vérifier. Le commentaire `update.rs:9-10` l'assume : « quiconque peut
  publier une release sur ce dépôt peut distribuer une mise à jour ».
- `README.md:29`, `README.md:188` : les exécutables ne sont pas signés.
- `.github/workflows/build.yml:19`, `:76`, `:96`, `:116`, `:130` : actions référencées par tag
  (`@v4`), pas par SHA ; `build.yml:23`, `:100` : toolchain `stable` non épinglée.
- `.github/workflows/build.yml:133` : le commentaire parle de « copies envoyées à la
  signature », mais aucune étape de signature n'existe dans le workflow.

Conséquence : la compromission du compte, d'un jeton ou d'une action tierce suffit à faire
exécuter du code chez tous les utilisateurs qui cliquent sur « Installer la version … ».

Recommandation : signer les artefacts hors du dépôt (Authenticode via SignPath, déjà préparé
d'après l'historique, puis `WinVerifyTrust` avant installation ; ou signature détachée
Ed25519 dont la clé publique est compilée dans l'application). En attendant : épingler les
actions par SHA, publier une attestation de provenance (`actions/attest-build-provenance`),
protéger les tags `v*`.

### S2. Mise à jour MSI : substitution possible entre vérification et installation — moyenne

- `src/bin/omnictl-gui/update.rs:99-111` : le MSI est vérifié en mémoire, puis écrit dans
  `%TEMP%\omnictl-<arch>.msi` (nom fixe), puis `msiexec /i … /passive` est lancé. Le paquet
  est `Scope="perMachine"` (`packaging/omnictl.wxs:15`), donc l'installation s'exécute avec
  élévation, après une invite UAC pendant laquelle le fichier reste modifiable par tout
  processus de l'utilisateur.

Recommandation : écrire dans un dossier fraîchement créé au nom aléatoire, garder un handle
ouvert sans partage en écriture jusqu'à la fin de `msiexec`, et à terme faire vérifier une
signature du MSI par Windows (voir S1).

### S3. Import de configuration : suppression avant écriture, entrée `[apps]` acceptée — moyenne

- `src/store.rs:283-294` : `import_dir` supprime tous les fichiers existants (`:287-289`)
  avant d'écrire les nouveaux (`:290-292`). Une erreur d'écriture en cours de route laisse une
  configuration vide ou partielle, alors que le commentaire `:281-282` annonce « tout est
  vérifié avant d'écrire quoi que ce soit ».
- `src/store.rs:220-226` : `entry_ok("apps")` renvoie vrai. Or `apps` est le dossier créé en
  `:286` ; `std::fs::write(dir.join("apps"), …)` échoue donc, après la suppression. Un fichier
  d'export contenant `[apps]` (malveillant ou simplement abîmé) efface la configuration.
  (Déduit de la lecture ; non exécuté.)

Recommandation : refuser les noms `apps` et les doublons dans `entry_ok`/`parse_export` ;
écrire le nouvel ensemble dans un dossier temporaire voisin puis permuter par renommage, ou
au minimum sauvegarder l'ancien avant suppression. Ajouter le cas `[apps]` aux tests
`hostile_exports_are_rejected`.

### S4. Identifiants de LED arbitraires acceptés — faible

- `src/keys.rs:217-219` : `resolve("0x..")` accepte n'importe quel octet, sans vérifier qu'il
  figure dans `KEYS`. Atteignable depuis le MCP (`src/mcp.rs:306`) et la ligne de commande
  (`src/main.rs:279`), puis envoyé au clavier (`src/proto.rs:489-508`) et mémorisé
  (`src/store.rs:131-141`).

L'effet d'un identifiant inconnu sur le firmware n'est pas établi **(hypothèse)**.
Recommandation : n'accepter que les identifiants présents dans `KEYS`, au moins côté MCP.

### S5. Programmes système lancés par nom nu — faible

- `src/bin/omnictl-gui/update.rs:104` (`msiexec`), `src/bin/omnictl-gui/install.rs:126-127`
  avec `src/bin/omnictl-gui/win.rs:1108-1111` (`cmd.exe`).

`std::process::Command` cherche d'abord dans le dossier de l'application sous Windows
**(hypothèse, d'après la documentation de la bibliothèque standard)** ; pour une copie lancée
depuis Téléchargements ou un dossier portable, un `msiexec.exe` ou `cmd.exe` déposé à côté
serait exécuté. Recommandation : chemins absolus construits depuis `%SystemRoot%\System32`.

### S6. Lecture globale de l'état des touches — faible (information)

- `src/keys.rs:275-294` : `GetAsyncKeyState` sur toutes les touches, en continu quand la
  réaction à la frappe est active. Rien n'est enregistré ni transmis (vérifié : le résultat ne
  sert qu'à `src/bin/omnictl-gui/app.rs:630-643`), mais ce motif, dans un exécutable non signé,
  ressemble à celui d'un enregistreur de frappe pour un antivirus. De plus le bit 0 utilisé
  (`keys.rs:290`) est documenté par Microsoft comme non fiable.

Recommandation : le documenter dans le README, et préférer Raw Input (voir P1).

### S7. Vérification réseau au démarrage sans réglage — faible

- `src/bin/omnictl-gui/app.rs:2469-2477` : requête vers GitHub à chaque lancement, y compris
  en mode portable et au démarrage automatique `--background`. Aucun réglage ne la désactive.
- `src/bin/omnictl-gui/win.rs:918-925` : `URLDownloadToFileW` sans délai maximal ni taille
  maximale.

Recommandation : un réglage « vérifier les mises à jour » et un délai d'expiration.

## 2. Performance

### P1. Boucle à 200 Hz dès que la réaction à la frappe est active — moyenne

- `src/bin/omnictl-gui/app.rs:50` (`FRAME` = 5 ms), `:548-555` (`animated()` vrai dès que
  `react != 0`, même sans touche animée ni frappe), `:710-711`.
- À chaque pas : `keys::pressed()` (`app.rs:631`) parcourt les 105 entrées de `KEYS` avec un
  `MapVirtualKeyW` et un `GetAsyncKeyState` par touche (`src/keys.rs:281-293`), alloue un
  `Vec`, puis `GetLastInputInfo` (`app.rs:646`).

Soit de l'ordre de 40 000 appels user32 par seconde, en permanence, pour un programme destiné
à rester en arrière-plan sur un portable. Recommandation : passer à un modèle événementiel
(Raw Input `RegisterRawInputDevices` avec `RIDEV_INPUTSINK`) ; à défaut, précalculer une fois
la table scancode → touche virtuelle et retomber à 30-60 Hz quand aucune onde ni éclair n'est
en cours.

### P2. Rondes de sondage au lieu de notifications — moyenne

- `src/bin/omnictl-gui/app.rs:712-713`, `:744` : quand les couleurs par touche sont actives,
  `kb_reachable()` s'exécute chaque seconde, soit deux requêtes HID (`src/proto.rs:301-309`).
  L'effet de ce trafic sur la mise en veille du clavier sans fil n'est pas mesuré
  **(hypothèse)**.
- Clavier absent : chaque ronde appelle `Link::open` (`src/link.rs:84-85`), qui énumère et
  ouvre toutes les interfaces HID du système (`src/hid_windows.rs:169-247`, `CreateFileW` en
  `:209-217` avant le filtre sur le VID en `:227`).
- L'application au premier plan est sondée deux fois par seconde par deux chemins distincts :
  `app.rs:647-650` (moteur) et le minuteur 2 (`app.rs:2540`, `:1359-1367`).
- Le minuteur 3 (`app.rs:2541`, 50 ms) tourne même fenêtre masquée.

Recommandation : `RegisterDeviceNotification` / `WM_DEVICECHANGE` pour l'arrivée et le départ
des périphériques, `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` pour l'application active,
filtrer sur `vid_0b05` dans le chemin avant d'ouvrir, arrêter le minuteur 3 quand la fenêtre
est masquée.

### P3. Relecture complète de l'état après chaque lot de commandes — moyenne à faible

- `src/bin/omnictl-gui/app.rs:835-841` : après tout lot, y compris purement local
  (`React`, `Gradient`, `Idle`, `AppSave`, `app.rs:376`), le fil relit le clavier (5 requêtes :
  `app.rs:432-457`) et la souris (6 requêtes : `src/mouse.rs:80-82`, `:109`, `:137-138`).
- Chaque requête attend jusqu'à 1 s (`src/link.rs:179`). Si la souris est en veille et que le
  dongle ne répond pas `FF AA`, chaque lot coûte une seconde **(hypothèse sur le comportement
  du dongle)**, car `dev.mouse` est remis à `None` et réessayé au lot suivant (`app.rs:476-482`).

Recommandation : ne relire que ce que la commande a pu modifier, espacer les essais sur un
appareil qui vient d'échouer.

### P4. Rendu GDI+ sans cache — faible

- `src/bin/omnictl-gui/win.rs:631-651` : contexte mémoire et bitmap recréés à chaque
  `WM_PAINT` ; `win.rs:282-283` : deux familles de police recherchées à chaque peinture ;
  `win.rs:395-422` : une police, un format et un pinceau créés puis détruits par texte.
- L'onglet « Touche par touche » dessine une centaine de touches et se repeint à 20 Hz quand
  l'aperçu est animé (`app.rs:2334-2337`, `:2541`).

Recommandation : conserver le tampon hors écran entre deux peintures (recréé sur `WM_SIZE`),
mettre en cache les polices par (taille, gras).

### P5. Le serveur MCP rouvre le périphérique à chaque appel — faible

- `src/mcp.rs:175-177`, appelé par chaque outil : une énumération HID complète plus une
  ouverture par appel. Recommandation : garder le `Keyboard` ouvert pour la session et le
  rouvrir sur erreur, comme le fait `Devices` dans l'interface (`app.rs:346-351`).

### P6. Petites inefficacités — faible

- Recherches linéaires dans des `Vec<(u8, T)>` au cœur de la boucle de rendu
  (`app.rs:637`, `:660`) : un tableau de 256 cases indexé par identifiant de LED, déjà
  utilisé pour `last`/`boost` (`app.rs:498-507`), conviendrait.
- `keys.txt` réécrit à chaque lot pendant un glissé du pinceau (`app.rs:796-801`).
- `Engine::new` lit `keys.txt` deux fois (`app.rs:526-527`).
- Une allocation par paquet HID (`src/hid_windows.rs:312`, `src/proto.rs:493`).

## 3. Design et architecture

### D1. État non partagé entre l'interface et la CLI / le MCP — moyenne

- `src/store.rs:131-141` (`update_colors`, CLI et MCP) : lecture, modification, réécriture du
  fichier. `src/bin/omnictl-gui/app.rs:796-801` : l'interface réécrit le fichier entier depuis
  sa copie en mémoire, chargée au démarrage (`app.rs:526-527`, `:2511`). Les couleurs posées
  par le MCP pendant que l'interface tourne sont donc perdues à la retouche suivante.
- Le moteur renvoie en continu les touches animées (`app.rs:685-695`) et tout le clavier au
  réveil (`app.rs:745-748`) : un `set_effect` ou `set_key_colors` venu du MCP est repeint
  par-dessus. L'interface sait couper le gardien avant d'appliquer un effet
  (`app.rs:1297-1304`) ; le MCP et la CLI non (`src/mcp.rs:275-279`, `src/main.rs:240-244`).
- `src/store.rs:58-64`, `:148-154` : écritures directes, non atomiques ; une coupure ou deux
  écrivains simultanés peuvent tronquer un fichier.

Recommandation : écrire par fichier temporaire puis renommage ; faire passer la CLI et le MCP
par l'instance graphique quand elle tourne (tube nommé ou `WM_COPYDATA`), ou au minimum
recharger `keys.txt` quand sa date change et exposer une commande qui suspend le moteur.

### D2. Le cœur du protocole n'a aucun test — moyenne

- `src/proto.rs`, `src/link.rs` et `src/mouse.rs` ne contiennent aucun test, alors qu'ils
  construisent tous les paquets (par exemple `proto.rs:402-484`, trois formats de paquets
  selon l'effet). Les 36 tests du dépôt couvrent JSON, stockage, effets calculés, MCP,
  analyse hidraw, versions.
- Cause : `Link` est lié en dur à `hid::Device` (`src/link.rs:74-80`), donc rien ne peut être
  testé sans matériel. `coalesce` (`app.rs:302-336`) et `Engine` (`app.rs:488-698`), pourtant
  logiques pures, ne sont pas testés non plus.
- Les tests de `install.rs` et `update.rs` ne tournent que sous Windows (modules sous
  `#[cfg(windows)]`, `src/bin/omnictl-gui/main.rs:6-13`).

Recommandation : un trait `Transport` (écrire / lire) implémenté par `hid::Device` et par un
faux en mémoire ; tests de paquets de référence tirés de `PROTOCOL.md` ; ajouter
`cargo clippy -- -D warnings` et `cargo fmt --check` à la CI.

### D3. `app.rs` : 2 543 lignes, moteur enfermé dans le binaire graphique — moyenne

- `src/bin/omnictl-gui/app.rs` réunit le fil de travail, le moteur d'animation, les widgets,
  cinq pages et la procédure de fenêtre.
- `Engine` appelle directement `win::idle_seconds` (`app.rs:564`), `win::foreground_exe`
  (`app.rs:602`) et `keys::pressed` (`app.rs:631`) : il n'est ni testable ni réutilisable.
  C'est pourquoi le gardien, les effets par touche animés et les profils par application
  n'existent pas en ligne de commande ni sous Linux (`README.md:164-165`).

Recommandation : déplacer `Engine` et `Devices` dans la bibliothèque derrière un petit trait
« environnement » (touches enfoncées, inactivité, application active), découper `app.rs` en
`worker.rs`, `widgets.rs` et un module par page.

### D4. Bug : la touche F est exclue des lettres gravées — moyenne

- `src/keys.rs:184` : le filtre `!k.name.starts_with('f')`, destiné à écarter `f1`…`f12`,
  écarte aussi la touche nommée `f` (`src/keys.rs:95`).

Conséquences : sur une disposition où cette position porte une autre lettre (Dvorak, Bépo,
Colemak), `resolve` (`keys.rs:221-229`) ne trouve pas la lettre gravée et retombe sur le nom
positionnel, donc colore une autre touche ; la colonne « lettre gravée » de `omnictl list` et
de `list_keys` est vide pour cette touche. Sans effet visible en QWERTY et AZERTY, où la
lettre F est à sa place.

Recommandation : filtrer sur les scancodes des touches de fonction (`0x3B..=0x44`, `0x57`,
`0x58`) ou sur `name.len() > 1 && name.starts_with('f')`, et ajouter un test.

### D5. Dialogues modaux pendant l'emprunt de l'état — faible

- `src/bin/omnictl-gui/app.rs:1618`, `:1627`, `:1629` : boîtes de dialogue ouvertes depuis
  `mouse_down`, lui-même appelé sous `with_app` (`app.rs:2319`), contrairement à la règle
  écrite en `src/bin/omnictl-gui/win.rs:773`.
- `with_app` ignore en silence tout message reçu pendant un emprunt (`app.rs:2303-2305`) :
  tant que le dialogue est ouvert, `WM_UPDATE` (`:2343-2346`) et le verrouillage de session
  (`:2347-2358`) sont perdus, et la fenêtre n'est plus repeinte (`:2309-2313`).

Recommandation : poster un message à soi-même et ouvrir le dialogue hors emprunt, comme pour
le menu de l'icône (`app.rs:2404-2411`).

### D6. Linux : une collection par `hidraw` supposée — faible (hypothèse)

- `src/hid_linux.rs:41-63` ne retient que la première « usage page » du descripteur, et
  `src/link.rs:96-98`, `:114-116` cherchent `FF00` et `FF01` comme deux nœuds distincts.
  Sous Linux, un nœud `hidraw` correspond à une interface USB, qui peut regrouper plusieurs
  collections ; si le dongle expose `FF00` et `FF01` sur la même interface, la souris serait
  introuvable et `Link::read` (`src/link.rs:160-173`), qui ne filtre pas sur le report ID,
  mélangerait les réponses. Le fichier annonce lui-même « non essayé sur du matériel »
  (`hid_linux.rs:7-8`). À vérifier avec `lsusb -v` sur le dongle.

### D7. Duplication — faible

- Conversions HSV copiées : `src/bin/omnictl-gui/app.rs:176-207` et `src/keyfx.rs:152-183`
  (publiques, donc réutilisables).
- Construction de la carte de LED identique dans `src/main.rs:262-285` et
  `src/mcp.rs:288-313` ; tables veille / économie d'énergie dans `src/main.rs:350-368` et
  `src/mcp.rs:353-369` ; affichage de l'état dans `src/main.rs:197-219` et
  `src/mcp.rs:201-232`.
- Le motif « trouver l'identifiant ou ajouter » est réécrit six fois (`main.rs:264`,
  `mcp.rs:290`, `store.rs:135`, `app.rs:320`, `app.rs:581`, `app.rs:1334`).
- `ARCH` défini dans `update.rs:16` et `install.rs:12`.
- La liste des modes du catalogue MCP (`src/mcp.rs:23-25`) est tenue à la main à côté de
  `proto::MODES` (`src/proto.rs:111-122`) sans test de cohérence.

Recommandation : un type `LedMap` dans la bibliothèque, des fonctions `parse_sleep` /
`parse_saving` et un rendu d'état communs.

### D8. Erreurs ignorées — faible

- Écritures de réglages dont l'échec est jeté : `src/bin/omnictl-gui/app.rs:1319`, `:1327`,
  `:1370`, `:1596`, `:1603`, `:1682` ; résultat de `set_autostart` ignoré en `:1329` (le
  gardien paraît actif alors que le démarrage automatique n'est pas inscrit).
- `app.rs:1649-1651` avec `:1416-1418` : `updating` n'est remis à faux que sur erreur ; un
  téléchargement qui ne rend jamais la main bloque le bouton jusqu'au redémarrage.
- `app.rs:2437-2447` : après une mise à jour, la nouvelle instance attend 1,5 s fixe ; si
  l'ancienne n'a pas encore libéré son verrou, elle se ferme et l'application ne redémarre pas.
- Codes de retour GDI+ jamais contrôlés (`win.rs:274-285`).

### D9. Incohérences et limites — faible

- Alerte batterie : 0 à 50 % dans l'interface (`app.rs:1486`, `:2151`), 0 à 100 % ailleurs
  (`src/main.rs:373-376`, `src/mcp.rs:370-374`, `src/proto.rs:358-363`).
- `set_profile` ne borne pas sa valeur dans la bibliothèque (`src/proto.rs:366-368`),
  contrairement à `set_sleep` / `set_saving`.
- Liste des applications limitée aux sept premières, sans défilement (`app.rs:2104`) : les
  suivantes ne peuvent plus être retirées depuis l'interface.
- MCP : la version de protocole demandée est renvoyée telle quelle, même inconnue
  (`src/mcp.rs:123-126`) ; un message qui n'est pas un objet (lot JSON-RPC) est ignoré sans
  erreur (`src/mcp.rs:112-114`).
- Aucune gestion du clavier dans la fenêtre (pas de `WM_KEYDOWN` dans `app.rs:2307-2423`) :
  interface entièrement dessinée, inutilisable sans souris ni par un lecteur d'écran.
- `SP_DEVICE_INTERFACE_DETAIL_DATA_W.cbSize` fixé à 8 (`src/hid_windows.rs:187`) : faux en
  32 bits, où l'énumération ne trouverait rien, sans erreur de compilation.

### D10. Dette documentaire — faible

- `Cargo.toml:5`, `:8` : les accents ont été remplacés par `??` dans le fichier commité.
- Commentaires périmés : `src/proto.rs:76` et `src/keys.rs:16` (« interface graphique, à
  venir »), `src/mouse.rs:7-9` (annonce que le DPI n'est pas écrit, alors que `set_dpi`
  existe en `:121-133`).

## 4. Idées de fonctionnalités

1. **Souris en ligne de commande et dans le MCP.** `src/mouse.rs` sait déjà lire l'état et
   régler éclairage, DPI et fréquence ; ni `src/main.rs` ni `src/mcp.rs` ne l'exposent.
   Commandes `omnictl mouse info|light|dpi|polling|save` et outils MCP équivalents.
2. **Mode démon sans fenêtre (`omnictl daemon`).** Une fois `Engine` dans la bibliothèque
   (D3) : gardien, effets par touche animés et atténuation disponibles sous Linux et pour qui
   ne veut pas de l'interface. Règle aussi D1 si la CLI et le MCP s'adressent au démon.
3. **Notification de batterie faible.** L'état est déjà relu toutes les 20 s
   (`app.rs:46`) et la structure de l'icône prévoit la bulle (`win.rs:673-676`) : prévenir
   quand le clavier ou la souris passe sous un seuil, et afficher le niveau dans l'icône.
4. **Profils nommés de couleurs par touche.** Aujourd'hui une carte générale plus une par
   application. Plusieurs cartes nommées, choisies depuis le menu de l'icône, la CLI
   (`omnictl apply <nom>`) ou le MCP.
5. **MCP plus complet.** Lire la carte mémorisée (`get_key_colors`), poser un effet par touche
   (`KeyFx`) et le dégradé, et une sortie `--json` pour `omnictl info` afin de l'utiliser dans
   des scripts.
6. **Éclairage piloté par l'état du système.** Touches témoins (Verr. maj, Verr. num),
   niveau de batterie sur la rangée de fonctions, clignotement sur notification.
7. **Remappage, macros et fonction de la molette.** Commandes déjà identifiées d'après
   `README.md:149`.
8. **Détection à chaud des périphériques** (voir P2) et réglage pour désactiver la
   vérification des mises à jour (voir S7).
9. **Accessibilité** : navigation au clavier (Tab, flèches, Entrée) et liste d'applications
   défilante (voir D9).
10. **Signature et vérification des mises à jour** (voir S1), prérequis pour une diffusion
    plus large et pour Smart App Control.
