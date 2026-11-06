use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: create_workspace <directory>")?;
    let workspace = gc_core::Workspace::create(Path::new(&path))?;
    println!("{}", workspace.root().display());
    Ok(())
}
