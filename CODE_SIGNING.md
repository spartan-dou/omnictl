# Politique de signature du code

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

*Signature du code offerte par SignPath.io, certificat de la SignPath Foundation.*

## Ce qui est signé

Les fichiers Windows publiés sur la page [Releases](../../releases) : `omnictl-gui-*.exe`,
`omnictl-*.exe`, les paquets `omnictl-*.msi` et les exécutables contenus dans les zips
portables. Ils sont compilés par [GitHub Actions](.github/workflows/build.yml) à partir du code
de ce dépôt, sans intervention manuelle, puis envoyés à SignPath par cette même compilation.
Rien de ce qui n'a pas été construit ainsi n'est signé.

## Rôles

- Auteurs et relecteurs : les contributeurs du dépôt ; toute modification passe par le
  dépôt GitHub public.
- Approbation : [@spartan-dou](https://github.com/spartan-dou) approuve chaque demande de
  signature, pour les seules versions publiées (tags `v…`).

Tous les membres de l'équipe utilisent l'authentification à deux facteurs sur GitHub et sur
SignPath.

## Vie privée

omnictl ne transmet aucune donnée à qui que ce soit. La seule connexion réseau est la
vérification des mises à jour, qui lit le fichier `VERSION` de la dernière release sur
GitHub, et le téléchargement des nouveaux fichiers quand l'utilisateur clique sur
« Installer la version… ». La réaction à la frappe lit seulement quelle touche est enfoncée
pour l'éclairer : rien n'est enregistré ni envoyé.
