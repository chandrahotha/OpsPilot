//! Pilot CLI - `pilot [path] [--json]`
//!
//! Answers the first question a developer has about an unfamiliar directory:
//! *what is this project?* The scan is read-only and never executes project
//! commands (see "Pilot Prerequisite.md" sections 1 and 19).

use pilot_core::ScanResult;
use std::env;
use std::path::Path;
use std::process::ExitCode;

/// Exit code used for invalid usage
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let mut json = false;
    let mut target: Option<String> = None;

    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("pilot: unknown option `{other}`");
                print_usage();
                return ExitCode::from(USAGE_ERROR);
            }
            path => target = Some(path.to_string()),
        }
    }

    let path = target.unwrap_or_else(|| ".".to_string());

    // Normalize before scanning so that `pilot` in a project folder reports the
    // real directory name instead of ".".
    let root = match pilot_scanner::resolve_project_path(&path) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("pilot: could not resolve `{path}`: {error}");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    if !Path::new(&root).is_dir() {
        eprintln!("pilot: `{root}` is not a directory");
        return ExitCode::from(USAGE_ERROR);
    }

    let result = pilot_scanner::scan_project(&root);

    if json {
        match serde_json::to_string_pretty(&result) {
            Ok(rendered) => println!("{rendered}"),
            Err(error) => {
                eprintln!("pilot: could not render scan result: {error}");
                return ExitCode::from(1);
            }
        }
    } else {
        print_report(&result, &root);
    }

    ExitCode::SUCCESS
}

fn print_usage() {
    println!("pilot - Cross-Platform Project Operations Launcher");
    println!();
    println!("USAGE:");
    println!("    pilot [path] [--json]");
    println!();
    println!("ARGS:");
    println!("    <path>    Directory to scan (defaults to the current directory)");
    println!();
    println!("OPTIONS:");
    println!("    --json    Print the normalized project model as JSON");
    println!("    -h, --help    Print this help");
}

/// Render the scan result as a human-readable report
fn print_report(result: &ScanResult, path: &str) {
    println!("Pilot - project scan");
    println!("Path:    {path}");

    let Some(model) = result.model() else {
        println!();
        println!("No project detected in this directory.");
        return;
    };

    println!("Project: {}", model.project.name);
    println!();

    if let Some(frontend) = &model.frontend {
        println!(
            "  {:<12} {:<14} :{}",
            "Frontend", frontend.framework, frontend.port
        );
    }
    if let Some(backend) = &model.backend {
        println!(
            "  {:<12} {:<14} :{}",
            "Backend", backend.framework, backend.port
        );
    }
    if let Some(database) = &model.database {
        println!(
            "  {:<12} {:<14} :{}",
            "Database", database.r#type, database.port
        );
    }
    if let Some(orm) = &model.orm {
        println!("  {:<12} {}", "ORM", orm.r#type);
    }
    if let Some(docker) = &model.docker {
        let kind = if docker.compose { "compose" } else { "dockerfile" };

        println!("  {:<12} detected ({kind})", "Docker");
    }
    if let Some(environment) = &model.environment {
        let mut files = Vec::new();
        if environment.env_file {
            files.push(".env");
        }
        if environment.env_example {
            files.push(".env.example");
        }
        if environment.env_local {
            files.push(".env.local");
        }

        println!("  {:<12} {}", "Environment", files.join(", "));
    }

    if !result.evidence.is_empty() {
        println!();
        println!("Evidence");
        for line in &result.evidence {
            println!("  - {line}");
        }
    }
}
