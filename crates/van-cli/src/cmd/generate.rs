use anyhow::{bail, Result};
use std::fs;
use van_context::project::VanProject;

pub fn run() -> Result<()> {
    let project = VanProject::load_cwd()?;
    let files = project.collect_files()?;
    let page_entries = project.page_entries(&files);

    if page_entries.is_empty() {
        bail!("No pages found in src/pages/");
    }

    let all_mock = project.load_all_mock_data();

    // Create dist/ directory
    let dist_dir = project.dist_dir();
    if dist_dir.exists() {
        fs::remove_dir_all(&dist_dir)?;
    }
    fs::create_dir_all(&dist_dir)?;

    let mut count = 0;

    for entry in &page_entries {
        // entry is like "pages/index.van"
        let stem = entry
            .strip_prefix("pages/")
            .unwrap_or(entry)
            .strip_suffix(".van")
            .unwrap_or(entry);

        let page_key = format!("pages/{}", stem);
        let mock_data = all_mock
            .get(&page_key)
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let mock_json = serde_json::to_string(&mock_data)?;

        let html = van_compiler::compile_page(entry, &files, &mock_json)
            .map_err(|e| anyhow::anyhow!("Failed to compile {}: {}", entry, e))?;

        // Write output: index.van -> dist/index.html, other.van -> dist/other/index.html
        let output_path = if stem == "index" {
            dist_dir.join("index.html")
        } else {
            let page_dir = dist_dir.join(stem);
            fs::create_dir_all(&page_dir)?;
            page_dir.join("index.html")
        };

        fs::write(&output_path, &html)?;
        println!(
            "  {} -> {}",
            entry,
            output_path
                .strip_prefix(&project.root)
                .unwrap_or(&output_path)
                .display()
        );
        count += 1;
    }

    println!("\nGenerated {} page(s) in dist/", count);
    Ok(())
}
