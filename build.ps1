# Compile omnictl pour Windows x64 et Windows ARM64, sans Visual Studio.
# (Fichier volontairement en ASCII : Windows PowerShell 5.1 lit mal l'UTF-8 sans BOM.)
#
# Prerequis :
#   - rustup (https://rustup.rs) avec une toolchain *-pc-windows-gnullvm et les cibles
#     x86_64-pc-windows-gnullvm et aarch64-pc-windows-gnullvm ;
#   - llvm-mingw (https://github.com/mstorsjo/llvm-mingw/releases, archive "ucrt")
#     decompresse dans %USERPROFILE%\.llvm-mingw, ou designe par $env:LLVM_MINGW.
#
# llvm-mingw ne sert ici que de bibliotheques (CRT + bibliotheques d'import Windows) :
# l'edition de liens est faite par rust-lld, livre avec Rust. Aucun executable de
# llvm-mingw n'est lance, ce qui evite les blocages de Smart App Control.
#
# Options : -Debug (compilation non optimisee), -Test (lance aussi les tests unitaires).
param([switch]$Debug, [switch]$Test)

# 'Continue' : cargo ecrit sa progression sur stderr, ce que 'Stop' prendrait pour une
# erreur sous PowerShell 5.1. Les vrais echecs sont detectes via $LASTEXITCODE.
$ErrorActionPreference = 'Continue'

$root = $env:LLVM_MINGW
if (-not $root) {
    $root = (Get-ChildItem "$env:USERPROFILE\.llvm-mingw" -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name | Select-Object -Last 1).FullName
}
if (-not $root -or -not (Test-Path "$root\aarch64-w64-mingw32\lib\crt2.o")) {
    throw "llvm-mingw introuvable : decompressez-le dans $env:USERPROFILE\.llvm-mingw"
}
$root = $root -replace '\\', '/'
$rt = (Get-ChildItem "$root/lib/clang" -Directory | Sort-Object Name | Select-Object -Last 1).FullName -replace '\\', '/'
# llvm-mingw dans le chemin : build.rs (icone) est compile pour la machine hote, avec
# l'editeur de liens par defaut de la toolchain, qui est le clang de llvm-mingw.
$env:Path = "$env:USERPROFILE\.cargo\bin;$root/bin;$env:Path"

$mode = @()
$dir = 'debug'
if (-not $Debug) { $mode = @('--release'); $dir = 'release' }

Push-Location $PSScriptRoot
try {
    New-Item -ItemType Directory -Force dist | Out-Null
    foreach ($arch in 'x86_64', 'aarch64') {
        $t = "$arch-pc-windows-gnullvm"
        $lib = "$root/$arch-w64-mingw32/lib"
        # CRT de demarrage et bibliotheques viennent tous de llvm-mingw (meme version de
        # mingw-w64) : on desactive donc le CRT "self-contained" de Rust, incomplet pour ARM64.
        # "-lunwind" prendrait libunwind.dll.a (donc une dependance a libunwind.dll) : un
        # dossier place en tete de recherche et ne contenant que libunwind.a force le lien statique.
        $static = (New-Item -ItemType Directory -Force "target\static-libs\$arch").FullName -replace '\\', '/'
        Copy-Item "$lib/libunwind.a" $static -Force
        $flags = @(
            '-Clinker=rust-lld', '-Clinker-flavor=ld.lld', '-Clink-self-contained=no',
            "-Lnative=$static", "-Lnative=$lib",
            # crt2.o doit preceder les bibliotheques : passe en "-l" verbatim, il est place en
            # tete (un -Clink-arg arriverait en fin de ligne, apres libmingw32, d'ou un
            # symbole atexit defini deux fois pour l'executable graphique).
            '-ldylib:+verbatim=crt2.o', "-Clink-arg=$lib/crtbegin.o",
            "-Clink-arg=$rt/lib/windows/libclang_rt.builtins-$arch.a",
            "-Clink-arg=$lib/crtend.o"
        )
        # Separateur 0x1F : format attendu par CARGO_ENCODED_RUSTFLAGS (chemins avec espaces).
        $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
        # Les tests ne tournent que pour l'architecture de la machine de build.
        if ($Test -and $arch -eq 'x86_64' -and $env:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
            cargo test --target $t
            if ($LASTEXITCODE -ne 0) { throw "echec des tests" }
        }
        cargo build @mode --target $t
        if ($LASTEXITCODE -ne 0) { throw "echec de compilation pour $t" }
        $name = 'x64'
        if ($arch -eq 'aarch64') { $name = 'arm64' }
        Copy-Item "target\$t\$dir\omnictl.exe" "dist\omnictl-$name.exe" -Force
        Copy-Item "target\$t\$dir\omnictl-gui.exe" "dist\omnictl-gui-$name.exe" -Force
    }
    Get-ChildItem dist\*.exe | Select-Object Name, Length
} finally {
    Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    Pop-Location
}
