//! Создать демо-проект и собрать его: `cargo run -p vn-store --example make_demo -- <папка> [путь к плееру]`.
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let dir = PathBuf::from(args.next().ok_or("укажите папку для демо-проекта")?);
    let story = vn_store::demo::create(&dir)?;
    println!("проект: {}", dir.display());
    println!("веб-версия: {}", vn_store::export::export_web(&dir, &story)?.display());
    if let Some(player) = args.next() {
        println!("игра: {}", vn_store::export::export_desktop(&dir, &story, &PathBuf::from(player))?.display());
    }
    Ok(())
}
