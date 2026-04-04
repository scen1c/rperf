use std::env;
use std::path::Path;
use std::process::Command;
use std::process::exit;

fn main() {
    let mut args = env::args().skip(1);
    let Some(project_path) = args.next() else {
        eprintln!("Usage: rperf <project-path> [cargo args...]");
        eprintln!("Try rperf --help for more information.");
        return;
    };

    let cargo_args: Vec<String> = args.collect();

    if !Path::new(&project_path).join("Cargo.toml").exists() {
        eprintln!("Error: Cargo.toml doesnt exist in {}", &project_path);
        exit(1);
    }

    let stat = Command::new("cargo")
        .arg("run")
        .args(&cargo_args)
        .current_dir(&project_path)
        .status();

    match stat {
        Ok(stat) => {
            if stat.success() {
                println!("Successfully finished with status: {}", stat);
            } else {
                eprintln!("Cargo run failed with status: {}", stat);
                exit(stat.code().unwrap_or(1));
            }
        },
        Err(err) => {
            eprintln!("Error: {}", err);
            exit(1);
        }
    }
}