# omnictl

Contrôleur libre et léger pour périphériques **ASUS ROG** reliés par câble USB ou par le
dongle 2,4 GHz **ROG Omni** : clavier **ROG Strix Scope II 96 Wireless** et souris
**ROG Keris II Ace**. Écrit pour continuer à les utiliser pleinement là où Armoury Crate ne
fonctionne pas — en particulier sur Windows ARM64.

- Deux exécutables d'environ 400 Ko chacun, sans installation ni droits administrateur.
- Aucune dépendance : bibliothèque standard de Rust et appels directs aux API de Windows.
- Une interface graphique native au style de Windows 11, une ligne de commande, et un
  serveur MCP pour piloter le clavier depuis un assistant IA.

> Projet indépendant, sans lien avec ASUS. Le protocole a été établi par rétro-ingénierie à
> des fins d'interopérabilité (voir [PROTOCOL.md](PROTOCOL.md)).

## Téléchargement

Les exécutables sont sur la page [Releases](../../releases) :

| Fichier | Rôle |
|---|---|
| `omnictl-gui-x64.exe` / `omnictl-gui-arm64.exe` | interface graphique |
| `omnictl-x64.exe` / `omnictl-arm64.exe` | ligne de commande et serveur MCP |

Ils ne sont pas signés : voir [Smart App Control](#smart-app-control).

## Interface graphique

Un volet par appareil (clavier, souris), avec sa batterie.

**Clavier**

- *Éclairage* : les dix effets intégrés du clavier, leurs couleurs, vitesse, direction et
  luminosité. Appliqués en direct ; « Enregistrer dans le clavier » les rend permanents.
- *Touche par touche* : un pinceau qui pose sur chaque touche une couleur **et** un effet
  (fixe, respiration, cycle, vague, dégradé). Réaction à la frappe (éclair ou onde),
  atténuation automatique quand le PC est inactif ou verrouillé, vue du clavier en noir ou blanc.
- *Applications* : des couleurs différentes selon le logiciel au premier plan.
- *Réglages* : profil embarqué, mise en veille, économie d'énergie, alerte batterie,
  éclairage dynamique Windows.

**Souris** : effet, couleur et luminosité de la LED, et synchronisation avec le clavier.

Deux choses à savoir :

- **Les dix effets intégrés sont exécutés par le clavier.** Une fois enregistré, un effet
  tourne sans logiciel, y compris en Bluetooth ou branché sur une autre machine.
- **Tout ce qui est « touche par touche » est calculé par l'ordinateur.** Le clavier ne
  sait pas mémoriser une couleur par touche. Ces couleurs et leurs animations n'existent
  que tant qu'omnictl tourne : l'option « Garder ces couleurs » le laisse en arrière-plan
  (icône près de l'horloge, démarrage avec la session) et les renvoie à chaque réveil du clavier.

## Ligne de commande

```
omnictl info
omnictl static 00a0ff --brightness 60 --save
omnictl effect breathing rouge bleu --speed slow
omnictl effect wave --direction right
omnictl keys --base 101010 zqsd=rouge esc=blanc space=cyan arrows=orange
omnictl apply
omnictl sleep 10
omnictl help
```

`omnictl help` donne la liste complète des commandes, des effets et des options ;
`omnictl list` celle des noms de touches et de groupes. Les noms de touches sont positionnels
(ceux d'un QWERTY US), mais une lettre seule désigne la touche qui porte réellement cette
lettre dans la disposition active : sur un AZERTY, `a=rouge` colore bien la touche A.

## Serveur MCP

`omnictl mcp` parle le [Model Context Protocol](https://modelcontextprotocol.io) sur son
entrée/sortie standard ; aucun port réseau n'est ouvert. Outils exposés : `keyboard_status`,
`set_effect`, `set_key_colors`, `apply_saved_keys`, `list_keys`, `set_power`, `set_profile`,
`set_dynamic_lighting`.

Claude Code :

```
claude mcp add omnictl -- C:\chemin\vers\omnictl-x64.exe mcp
```

Claude Desktop (`claude_desktop_config.json`) :

```json
{
  "mcpServers": {
    "omnictl": { "command": "C:\\chemin\\vers\\omnictl-x64.exe", "args": ["mcp"] }
  }
}
```

## Ce qui est vérifié, et ce qui ne l'est pas

| Fonction | État |
|---|---|
| Clavier par USB et par le dongle : état, batterie, effets intégrés, réglages | vérifié sur le matériel (firmware 8.00.04, récepteur 7.00.08) |
| Couleurs et effets touche par touche, réaction à la frappe | vérifié (environ 15 à 20 images par seconde via le dongle) |
| Profils par application, atténuation automatique | enregistrement et interface vérifiés ; bascule en conditions réelles peu éprouvée |
| Souris : lecture de l'état | vérifié (firmware 3.00.17) |
| Souris : éclairage et synchronisation | écrit d'après le pilote ASUS, **pas encore essayé sur le matériel** |
| Souris : DPI, fréquence de scrutation, boutons | non pris en charge |
| Bluetooth | non pris en charge (le clavier n'y accepte que les effets intégrés, pas le touche par touche) |
| Remappage de touches, macros, réglage de la molette | commandes identifiées, non implémentées |
| Mise à jour de firmware | hors périmètre, volontairement |
| Linux, macOS | non pris en charge |

## Compilation

Sous Windows, avec la chaîne MSVC habituelle :

```
cargo build --release
```

Sans Visual Studio :

1. Installer [rustup](https://rustup.rs) avec une toolchain `stable-*-pc-windows-gnullvm` et les
   cibles `x86_64-pc-windows-gnullvm` et `aarch64-pc-windows-gnullvm`.
2. Décompresser [llvm-mingw](https://github.com/mstorsjo/llvm-mingw/releases) (archive « ucrt »)
   dans `%USERPROFILE%\.llvm-mingw`. Il ne sert que de bibliothèques ; l'édition de liens est
   faite par `rust-lld`, fourni avec Rust.
3. Lancer `powershell -ExecutionPolicy Bypass -File build.ps1 -Test`.

Les exécutables sont produits dans `dist\`.

### Smart App Control

Les exécutables ne sont pas signés. Sur une machine où Smart App Control est actif, Windows
peut refuser de les lancer, et son verdict varie d'une compilation à l'autre. Il n'existe
pas d'exception par application : il faut soit le désactiver (réglage définitif, Defender
et SmartScreen restent actifs), soit signer les exécutables.

## Matériel

| Appareil | Identifiant USB | Canal utilisé |
|---|---|---|
| Clavier, câble USB | `0B05:1AAE` | usage page `FF00`, rapports de 64 octets |
| Clavier, via le dongle Omni | `0B05:1ACE` | usage page `FF00`, report ID 2 + 63 octets |
| Souris, via le dongle Omni | `0B05:1ACE` | usage page `FF01`, report ID 3 + 63 octets |

## Licence et remerciements

GPL-2.0-or-later (voir [LICENSE](LICENSE)). La table des identifiants de LED et la base du
protocole d'éclairage viennent d'[OpenRGB](https://openrgb.org) (contrôleurs
`AsusAuraKeyboardController` et `AsusAuraMouseController`, Mola19 et contributeurs).
