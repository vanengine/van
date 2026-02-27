use anyhow::Result;
use std::fs;
use std::path::Path;
use van_context::project::VanProject;

pub fn run() -> Result<()> {
    let project = VanProject::load_cwd()?;
    let theme_name = &project.config.name;
    let files = project.collect_files()?;

    println!("Building theme '{}'...", theme_name);
    println!("  Collected {} source files", files.len());

    // Create dist/{theme}/ directory
    let dist_dir = project.dist_dir().join(theme_name);
    if dist_dir.exists() {
        fs::remove_dir_all(&dist_dir)?;
    }
    fs::create_dir_all(&dist_dir)?;

    let asset_prefix = format!("/themes/{}/assets", theme_name);
    let mut page_count = 0;
    let mut component_count = 0;
    let mut asset_count = 0;

    // Compile pages
    for entry in &project.page_entries(&files) {
        let result = van_compiler::compile_page_assets(entry, &files, "{}", &asset_prefix)
            .map_err(|e| anyhow::anyhow!("Failed to compile {}: {}", entry, e))?;

        let html_rel = entry.replace(".van", ".html");
        let html_path = dist_dir.join(&html_rel);
        if let Some(parent) = html_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&html_path, &result.html)?;
        println!("  {} -> {}", entry, html_rel);
        page_count += 1;

        for (asset_path, content) in &result.assets {
            write_asset(&dist_dir, asset_path, content, &asset_prefix)?;
            asset_count += 1;
        }
    }

    // Compile components
    for entry in &project.component_entries(&files) {
        let result = van_compiler::compile_page_assets(entry, &files, "{}", &asset_prefix)
            .map_err(|e| anyhow::anyhow!("Failed to compile {}: {}", entry, e))?;

        let html_rel = entry.replace(".van", ".html");
        let html_path = dist_dir.join(&html_rel);
        if let Some(parent) = html_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&html_path, &result.html)?;
        println!("  {} -> {}", entry, html_rel);
        component_count += 1;

        for (asset_path, content) in &result.assets {
            write_asset(&dist_dir, asset_path, content, &asset_prefix)?;
            asset_count += 1;
        }
    }

    println!(
        "\nBuild complete: {} page(s), {} component(s), {} asset(s) in dist/{}/",
        page_count, component_count, asset_count, theme_name
    );

    Ok(())
}

/// Write an asset file under dist_dir, stripping the asset_prefix from the path.
fn write_asset(
    dist_dir: &Path,
    asset_path: &str,
    content: &str,
    asset_prefix: &str,
) -> Result<()> {
    // asset_path is like "/themes/{theme}/assets/css/pages/index.abc123.css"
    // Strip asset_prefix to get "css/pages/index.abc123.css"
    let rel = asset_path
        .strip_prefix(asset_prefix)
        .unwrap_or(asset_path)
        .trim_start_matches('/');
    let out_path = dist_dir.join("assets").join(rel);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, content)?;
    Ok(())
}
