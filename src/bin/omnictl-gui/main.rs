//! Interface graphique d'omnictl. Elle n'existe que pour Windows : elle est dessinée
//! directement avec les API du système (voir `app.rs` et `win.rs`).

#![windows_subsystem = "windows"]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod update;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    app::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("L'interface graphique d'omnictl n'existe que pour Windows ; utilisez la commande `omnictl`.");
    std::process::exit(1);
}
