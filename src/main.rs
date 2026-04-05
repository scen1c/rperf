use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, exit};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    target_directory: PathBuf,
    packages: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    manifest_path: PathBuf,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);

    let Some(project_path_raw) = args.next() else {
        print_usage();
        return Ok(());
    };

    if project_path_raw == "--help" || project_path_raw == "-h" {
        print_usage();
        return Ok(());
    }

    let run_args: Vec<String> = args.collect();
    let project_path = PathBuf::from(project_path_raw);
    let manifest_path = project_path.join("Cargo.toml");

    if !manifest_path.exists() {
        return Err(format!(
            "Cargo.toml does not exist in {}",
            project_path.display()
        ));
    }

    let canon_manifest = fs::canonicalize(&manifest_path)
        .map_err(|e| format!("cannot resolve project path {}: {e}", project_path.display()))?;

    let build_status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .current_dir(&project_path)
        .status()
        .map_err(|e| format!("failed to start cargo build: {e}"))?;

    if !build_status.success() {
        return Err("build failed".to_string());
    }

    let binary_path = resolve_binary_path(&project_path, &canon_manifest)?;

    let mut child = Command::new(&binary_path)
        .args(&run_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start binary {}: {e}", binary_path.display()))?;

    let pid = Pid::from_u32(child.id());
    let mut system = System::new();
    let start = Instant::now();

    let mut peak_cpu = 0.0f32;
    let mut cpu_sum = 0.0f32;
    let mut samples = 0u32;

    println!("Started: {}", binary_path.display());
    println!("PID: {}", child.id());

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let duration = start.elapsed();
                println!("Program status: {status}");

                if samples > 0 {
                    println!("Average CPU usage: {:.2}%", cpu_sum / samples as f32);
                    println!("Peak CPU usage: {:.2}%", peak_cpu);
                } else {
                    println!("No CPU samples collected.");
                }

                println!("Execution time: {:.3?}", duration);

                if !status.success() {
                    exit(status.code().unwrap_or(1));
                }

                return Ok(());
            }
            Ok(None) => {
                system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

                if let Some(process) = system.process(pid) {
                    let cpu = process.cpu_usage();
                    cpu_sum += cpu;
                    samples += 1;
                    if cpu > peak_cpu {
                        peak_cpu = cpu;
                    }
                }

                thread::sleep(Duration::from_millis(500));
            }
            Err(err) => {
                return Err(format!("failed while waiting for process: {err}"));
            }
        }
    }
}

fn resolve_binary_path(project_path: &Path, canonical_manifest: &Path) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .current_dir(project_path)
        .output()
        .map_err(|e| format!("failed to run cargo metadata: {e}"))?;

    if !output.status.success() {
        return Err("cargo metadata failed".to_string());
    }

    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse cargo metadata output: {e}"))?;

    let package = metadata
        .packages
        .into_iter()
        .find(|pkg| pkg.manifest_path == canonical_manifest)
        .ok_or_else(|| "could not find matching package in cargo metadata".to_string())?;

    let bin_target = package
        .targets
        .into_iter()
        .find(|t| t.kind.iter().any(|k| k == "bin"))
        .ok_or_else(|| "project has no binary target".to_string())?;

    let mut binary_name = bin_target.name;
    if cfg!(windows) {
        binary_name.push_str(".exe");
    }

    Ok(metadata.target_directory.join("debug").join(binary_name))
}

fn print_usage() {
    eprintln!("Usage: rperf <project-path> [binary args...]");
    eprintln!("Example: rperf /path/to/project --help");
}