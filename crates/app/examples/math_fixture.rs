//! Private native UI fixture; does not use the user's notes or load OCR models.
use folio_app::Controller;
use folio_document::Point;
fn main() -> Result<(), String> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Provide an empty fixture directory")?,
    );
    if root.join("notes.sqlite3").exists() {
        return Err("Fixture exists".into());
    }
    let mut app = Controller::open(root)?;
    app.rename("Math solver verification".into());
    app.add_text("2x+3=11".into(), Point::new(80., 100.));
    app.flush()?;
    println!("{}", app.active);
    Ok(())
}
