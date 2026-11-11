# omnictl

Contrôleur libre et léger pour le clavier **ASUS ROG Strix Scope II 96 Wireless**, écrit pour
continuer à l'utiliser pleinement là où Armoury Crate ne fonctionne pas — en particulier sur
Windows ARM64.

- Un seul exécutable d'environ 400 Ko, sans installation, sans service résident.
- Aucune dépendance : bibliothèque standard de Rust et appels directs à l'API HID de Windows.
- Fonctionne par **câble USB** et par le **dongle 2,4 GHz ROG Omni**.
- Utilisable en ligne de commande, ou par un assistant IA via un **serveur MCP** intégré.

> Projet indépendant, sans lien avec ASUS. Le protocole a été établi par rétro-ingénierie à
> des fins d'interopérabilité (voir [PROTOCOL.md](PROTOCOL.md)).

## État du projet

| Fonction | État |
|---|---|
| État, firmware, batterie, réglages (USB) | vérifié sur le matériel |
| Effets lumineux intégrés (USB) | statique et respiration vérifiés visuellement ; les autres sont acquittés par le clavier mais pas encore tous contrôlés à l'œil |
| Couleurs touche par touche (USB et dongle) | vérifié visuellement |
| Lecture d'état et effet statique via le dongle | vérifié |
| Veille, économie d'énergie, alerte batterie, profil, éclairage dynamique Windows (écriture) | implémenté d'après le pilote ASUS, **pas encore essayé sur le matériel** |
| Serveur MCP | protocole testé ; appels matériels à refaire clavier réveillé |
| Interface graphique native | à faire |
| Bluetooth | non pris en charge (le pilote ASUS n'y permet que les effets intégrés, pas le touche par touche) |
| Remappage de touches, macros, réglage de la molette | commandes identifiées, non implémentées |

## Utilisation

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
`omnictl list` celle des noms de touches et de groupes.

Deux choses à savoir :

- **Les effets sont exécutés par le clavier.** Une fois enregistré (`--save`), un effet tourne
  sans logiciel, y compris en Bluetooth ou branché sur une autre machine.
- **Les couleurs touche par touche sont tenues par l'ordinateur.** Le clavier ne sait pas les
  mémoriser : il les perd à la mise en veille ou à l'extinction. La dernière carte appliquée
  est gardée dans `%APPDATA%\omnictl\keys.txt` et `omnictl apply` la remet en place.

Les noms de touches sont positionnels (ceux d'un QWERTY US), mais une lettre seule désigne la
touche qui porte réellement cette lettre dans la disposition active : sur un AZERTY, `a=rouge`
colore bien la touche A.

### Éclairage dynamique Windows

Le clavier expose aussi l'interface standard HID LampArray. Avec `omnictl wdl on`, Windows 11
peut piloter l'éclairage lui-même (Paramètres → Personnalisation → Éclairage dynamique), sans
aucun logiciel tiers.

## Serveur MCP

`omnictl mcp` parle le [Model Context Protocol](https://modelcontextprotocol.io) sur son
entrée/sortie standard ; aucun port réseau n'est ouvert. Outils exposés : `keyboard_status`,
`set_effect`, `set_key_colors`, `apply_saved_keys`, `list_keys`, `set_power`, `set_profile`,
`set_dynamic_lighting`.

Claude Code :

```
claude mcp add omnictl -- C:\chemin\vers\omnictl.exe mcp
```

Claude Desktop (`claude_desktop_config.json`) :

```json
{
  "mcpServers": {
    "omnictl": { "command": "C:\\chemin\\vers\\omnictl.exe", "args": ["mcp"] }
  }
}
```

## Compilation

Sous Windows, sans Visual Studio :

1. Installer [rustup](https://rustup.rs) avec une toolchain `stable-*-pc-windows-gnullvm` et les
   cibles `x86_64-pc-windows-gnullvm` et `aarch64-pc-windows-gnullvm`.
2. Décompresser [llvm-mingw](https://github.com/mstorsjo/llvm-mingw/releases) (archive « ucrt »)
   dans `%USERPROFILE%\.llvm-mingw`. Il ne sert que de bibliothèques ; l'édition de liens est
   faite par `rust-lld`, fourni avec Rust.
3. Lancer :

```
powershell -ExecutionPolicy Bypass -File build.ps1 -Test
```

Les exécutables sont produits dans `dist\` : `omnictl-x64.exe` et `omnictl-arm64.exe`.

### Smart App Control

Les exécutables ne sont pas signés. Sur une machine où Smart App Control est actif, Windows
peut refuser de les lancer, et son verdict varie d'une compilation à l'autre. Seule une
signature de code reconnue par Windows règle la question durablement.

## Matériel

| Mode | Identifiant USB | Canal utilisé |
|---|---|---|
| Câble USB | `0B05:1AAE` | usage page `FF00`, rapports de 64 octets |
| Dongle ROG Omni | `0B05:1ACE` | usage page `FF00`, report ID 2 + 63 octets |

Le dongle Omni peut aussi porter une souris sur un second canal (`FF01`) ; omnictl n'y touche pas.

## Licence et remerciements

GPL-2.0-or-later (voir [LICENSE](LICENSE)). La table des identifiants de LED et la base du
protocole d'éclairage viennent d'[OpenRGB](https://openrgb.org) (contrôleur
`AsusAuraKeyboardController`, Mola19 et contributeurs).
