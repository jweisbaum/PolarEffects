//! Small deterministic fleet for native desktop comparison checks.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = pe_core::io::load(std::path::Path::new(
        "tools/webdriver/fixtures/analysis.wpsproj",
    ))?;
    root.name = "Comparison fleet".into();
    root.boat.name = "Alpha".into();
    for name in ["Bravo", "Charlie", "Delta"] {
        let mut boat = root.clone();
        boat.boat_tabs.clear();
        boat.id = pe_core::Project::new(name, pe_core::Boat::default(), root.created).id;
        boat.name = name.into();
        boat.boat.name = name.into();
        root.boat_tabs.push(boat);
    }
    pe_core::io::save(
        &root,
        std::path::Path::new("tools/webdriver/fixtures/boats.wpsproj"),
    )?;
    Ok(())
}
