use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Input;
use std::fs;
use std::path::{Path, PathBuf};
use van_context::config::VanConfig;

/// Run the interactive `van init` command.
pub fn run(name: Option<String>) -> Result<()> {
    println!();
    println!(
        "  {}",
        style("Van - Create a new project").bold().cyan()
    );
    println!();

    // Prompt for project name if not provided
    let project_name = match name {
        Some(n) => n,
        None => Input::new()
            .with_prompt(format!("  {}", style("Project name").bold()))
            .interact_text()
            .context("Failed to read project name")?,
    };

    // Validate project name
    if project_name.is_empty() {
        bail!("Project name cannot be empty");
    }
    if project_name
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '-' && c != '_')
    {
        bail!("Project name can only contain alphanumeric characters, hyphens, and underscores");
    }

    let project_dir = PathBuf::from(&project_name);

    // Check if directory already exists
    if project_dir.exists() {
        bail!("Directory '{}' already exists", project_name);
    }

    // Scaffold the project
    println!();
    println!(
        "  {} {}",
        style("Scaffolding project in").dim(),
        style(format!("./{project_name}/")).dim().bold()
    );
    println!();

    let files =
        scaffold_project(&project_dir, &project_name).context("Failed to scaffold project")?;

    // Print created files
    for file in &files {
        println!("  {}  {}", style("+").green().bold(), style(file).dim());
    }

    // Done message
    println!();
    println!(
        "  {} Project created successfully.",
        style("Done.").green().bold()
    );
    println!();
    println!("  Now run:");
    println!();
    println!("    {}  {}", style("cd").cyan(), project_name);
    println!("    {}", style("van dev").cyan());
    println!();

    Ok(())
}

/// Scaffold a new Van project with starter files.
pub fn scaffold_project(project_dir: &Path, name: &str) -> Result<Vec<String>> {
    let mut created_files = Vec::new();

    // Create directory structure
    let dirs = [
        "src/pages",
        "src/components",
        "src/layouts",
        "src/assets",
        "data",
    ];
    for dir in &dirs {
        fs::create_dir_all(project_dir.join(dir))
            .with_context(|| format!("Failed to create directory: {dir}"))?;
    }

    // package.json
    let config = VanConfig::new(name);
    let config_path = project_dir.join("package.json");
    fs::write(&config_path, config.to_json_pretty()?)?;
    created_files.push("package.json".into());

    // src/pages/index.van
    fs::write(
        project_dir.join("src/pages/index.van"),
        include_str!("templates/pages/index.van"),
    )?;
    created_files.push("src/pages/index.van".into());

    // src/components/hello.van
    fs::write(
        project_dir.join("src/components/hello.van"),
        include_str!("templates/components/hello.van"),
    )?;
    created_files.push("src/components/hello.van".into());

    // src/layouts/default.van
    fs::write(
        project_dir.join("src/layouts/default.van"),
        include_str!("templates/layouts/default.van"),
    )?;
    created_files.push("src/layouts/default.van".into());

    // data/index.json
    fs::write(
        project_dir.join("data/index.json"),
        include_str!("templates/data/index.json"),
    )?;
    created_files.push("data/index.json".into());

    // .gitignore
    fs::write(
        project_dir.join(".gitignore"),
        "dist/\nnode_modules/\n.van/\n",
    )?;
    created_files.push(".gitignore".into());

    Ok(created_files)
}
