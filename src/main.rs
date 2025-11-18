use anyhow::{Result, Context};
use serde_json::{Value, Number};

mod manifest;
mod template_loader;
mod renderer;
mod hooks;
mod vcs;
mod util;

use manifest::{load_manifest, Manifest, VarKind};
use template_loader::{load_template, template_root, copy_to_temp_root};
use dialoguer::{Input, Confirm};
use std::collections::BTreeMap;
use std::io;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::env;
use hooks::{run_pre_prompt, run_pre_gen, run_post_gen};
use crate::util::{sanitize_slug_python, is_safe_rel_path, safe_resolve_under_canon};
use manifest::CopyFilter;

fn main() -> Result<()> {
    let (source, output) = parse_args().map_err(|e| {
        eprintln!("Error: {}", e);
        e
    })?;
    run(source, output).map_err(|e| {
        eprintln!("Error: {}", e);
        e
    })
}

fn run(source: String, output: PathBuf) -> Result<()> {
    let ts = load_template(&source)?;
    let original_root = template_root(&ts);
    // Auto-detect and prepare Git submodules in source repository (best-effort)
    if vcs::has_gitmodules(original_root) {
        if let Err(e) = vcs::git_submodule_sync(original_root, true) {
            eprintln!("Warning: submodule sync failed: {}", e);
        }
        if let Err(e) = vcs::git_submodule_update_init(original_root, true, None) {
            eprintln!("Warning: submodule update --init failed: {}", e);
        }
    }
    // Auto-detect and update SVN working copy (best-effort)
    if vcs::has_svn_meta(original_root) {
        if let Err(e) = vcs::svn_update(original_root) {
            eprintln!("Warning: svn update failed: {}", e);
        }
    }
    // Step a) copy template to a temp directory
    let (temp_root_guard, temp_root) = copy_to_temp_root(original_root)?;
    let root = temp_root.as_path();
    let manifest: Manifest = load_manifest(root)?;
    let mut vars: BTreeMap<String, Value> = BTreeMap::new();

    // Pre-fill defaults
    for spec in &manifest.variables {
        if let Some(default) = &spec.default {
            vars.insert(spec.name.clone(), default.clone());
        }
    }

    // Run pre_prompt.lua to update defaults
    let initial_vars_json = serde_json::Value::Object(
        vars.iter().map(|(k,v)| (k.clone(), v.clone())).collect()
    );
    if let Some(updated) = run_pre_prompt(root, &initial_vars_json)? {
        if let Some(obj) = updated.as_object() {
            for (k, v) in obj.iter() { vars.insert(k.clone(), v.clone()); }
        }
    }

    // Evaluate Jinja defaults with dependency resolution before prompting
    vars = manifest.evaluate_defaults(&vars)?;

    // One-by-one TUI prompts (fallback to stdin when not a TTY)
    let is_tty = io::stdin().is_terminal();
    for spec in &manifest.variables {
        match &spec.kind {
            VarKind::String => {
                let def = vars.get(&spec.name).and_then(|v| v.as_str()).map(|s| s.to_string());
                let prompt = format!("Enter {}:", spec.name);
                let input: String = if is_tty {
                    if let Some(d) = def.clone() {
                        Input::new().with_prompt(format!("{} (default: {})", prompt, d)).allow_empty(true).interact_text()?
                    } else {
                        Input::new().with_prompt(&prompt).interact_text()?
                    }
                } else {
                    println!("{}{}", prompt, def.as_ref().map(|d| format!(" (default: {})", d)).unwrap_or_default());
                    let mut buf = String::new();
                    io::stdin().read_line(&mut buf)?;
                    buf.trim_end().to_string()
                };
                let final_value = if input.is_empty() { def.unwrap_or_default() } else { input };
                vars.insert(spec.name.clone(), Value::String(final_value));
            }
            VarKind::Bool => {
                let def = vars.get(&spec.name).and_then(|v| v.as_bool()).unwrap_or(false);
                let val = if is_tty {
                    Confirm::new()
                        .with_prompt(format!("{}?", spec.name))
                        .default(def)
                        .interact()?
                } else {
                    println!("{}? (y/n, default: {})", spec.name, if def { "y" } else { "n" });
                    let mut buf = String::new();
                    io::stdin().read_line(&mut buf)?;
                    let s = buf.trim().to_ascii_lowercase();
                    if s.is_empty() { def } else { s.starts_with('y') }
                };
                vars.insert(spec.name.clone(), Value::Bool(val));
            }
            VarKind::Number => {
                let def = vars.get(&spec.name).and_then(|v| v.as_i64());
                let prompt = format!("Enter number for {}:", spec.name);
                let input: String = if is_tty {
                    if let Some(d) = def {
                        Input::new().with_prompt(format!("{} (default: {})", prompt, d)).allow_empty(true).interact_text()?
                    } else {
                        Input::new().with_prompt(&prompt).interact_text()?
                    }
                } else {
                    println!("{}{}", prompt, def.map(|d| format!(" (default: {})", d)).unwrap_or_default());
                    let mut buf = String::new();
                    io::stdin().read_line(&mut buf)?;
                    buf.trim_end().to_string()
                };
                let final_num = if input.trim().is_empty() {
                    def.unwrap_or(0)
                } else {
                    input.trim().parse::<i64>().map_err(|e| anyhow::anyhow!("Failed to parse number: {}", e))?
                };
                vars.insert(spec.name.clone(), Value::Number(Number::from(final_num)));
            }
            VarKind::Choice(choices) => {
                // Display dictionary-style mapping: "value": "label"
                // Header: if name ends with "_code", show base name capitalized (language_code -> Language)
                let display_name = if spec.name.ends_with("_code") {
                    let base = spec.name.trim_end_matches("_code");
                    let mut chars = base.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                        None => spec.name.clone(),
                    }
                } else {
                    spec.name.clone()
                };
                println!("{}:", display_name);

                // Labels map is optional; fallback to echoing the value itself.
                let labels = spec.choice_labels.clone().unwrap_or_default();
                for c in choices.iter() {
                    let label = labels.get(c).cloned().unwrap_or_else(|| c.clone());
                    println!("  \"{}\": \"{}\"", c, label);
                }

                // Determine default value from current vars or first choice
                let default_val = if let Some(Value::String(d)) = vars.get(&spec.name) {
                    if choices.contains(d) { d.clone() } else { choices.first().cloned().unwrap_or_default() }
                } else { choices.first().cloned().unwrap_or_default() };

                let input: String = if is_tty {
                    Input::new()
                        .with_prompt(format!("Enter value (default: {})", default_val))
                        .allow_empty(true)
                        .interact_text()?
                } else {
                    println!("Enter value (default: {})", default_val);
                    let mut buf = String::new();
                    io::stdin().read_line(&mut buf)?;
                    buf.trim_end().to_string()
                };
                let picked = if input.trim().is_empty() {
                    default_val
                } else if choices.contains(&input.trim().to_string()) {
                    input.trim().to_string()
                } else {
                    eprintln!("Invalid value, using default.");
                    default_val
                };
                vars.insert(spec.name.clone(), Value::String(picked));
            }
        }
    }

    // Prepare a staging output directory inside temp for atomic rendering
    let staging = tempfile::tempdir()?;
    let staging_out = staging.path().join("out");
    std::fs::create_dir_all(&staging_out)?;

    // Run pre_gen_project.lua in temp context, targeting staging output
    let vars_json = serde_json::Value::Object(vars.iter().map(|(k,v)| (k.clone(), v.clone())).collect());
    let pre = run_pre_gen(root, &vars_json, &staging_out)?;
    // Ensure hook-created files are placed under the main project directory.
    let proj_slug = sanitize_slug_python(vars.get("project_slug").and_then(|v| v.as_str()).unwrap_or("project"));
    let proj_root = staging_out.join(&proj_slug);
    std::fs::create_dir_all(&proj_root)?;
    let proj_root_canon = proj_root.canonicalize()?;
    for (p, content) in pre.created_files {
        let p_str = p.to_string_lossy();
        if !is_safe_rel_path(&p_str) {
            anyhow::bail!(format!("Unsafe hook-created file path: {}", p_str));
        }
        let target = safe_resolve_under_canon(&proj_root_canon, &p)?;
        if let Some(parent) = target.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&target, content)?;
    }

    println!("Rendering templates...");

    let copy_filter: CopyFilter = manifest.compile_copy_filter()?;
    renderer::render_all(root, &staging_out, &vars, &copy_filter)?;

    // Run post_gen_project.lua (also targeting staging output)
    let vars_json2 = serde_json::Value::Object(vars.iter().map(|(k,v)| (k.clone(), v.clone())).collect());
    let post = run_post_gen(root, &vars_json2, &staging_out)?;
    // Post-gen files also go under the main project directory.
    let proj_root2 = staging_out.join(&proj_slug);
    std::fs::create_dir_all(&proj_root2)?;
    let proj_root2_canon = proj_root2.canonicalize()?;
    for (p, content) in post.created_files {
        let p_str = p.to_string_lossy();
        if !is_safe_rel_path(&p_str) {
            anyhow::bail!(format!("Unsafe hook-created file path: {}", p_str));
        }
        let target = safe_resolve_under_canon(&proj_root2_canon, &p)?;
        if let Some(parent) = target.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&target, content)?;
    }

    // Ensure final output root exists before secure resolution
    std::fs::create_dir_all(&output)?;
    let output_canon = output.canonicalize()?;

    // Step c) copy processed files from staging to final output
    for entry in walkdir::WalkDir::new(&staging_out).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path
            .strip_prefix(&staging_out)
            .with_context(|| format!("Failed to compute relative path from staging: {}", path.display()))?;
        let target = safe_resolve_under_canon(&output_canon, rel)?;
        if path.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::copy(path, &target)?;
        }
    }

    // Step d) delete temp dirs by dropping guards (TempDir cleans up on drop)
    drop(staging);
    drop(temp_root_guard);

    println!("Rendering complete!");
    println!("Generated successfully: {}", output.display());
    Ok(())
}

fn parse_args() -> Result<(String, PathBuf)> {
    let mut args = env::args().skip(1);
    let mut source: Option<String> = None;
    let mut output = PathBuf::from(".");

    // Default: first argument is SOURCE; optionally support "--output <dir>"
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-s" | "--source" => {
                if let Some(val) = args.next() { source = Some(val); }
                else { return Err(anyhow::anyhow!("Missing value for --source")); }
            }
            "-o" | "--output" => {
                if let Some(val) = args.next() { output = PathBuf::from(val); }
                else { return Err(anyhow::anyhow!("Missing value for --output")); }
            }
            _ => { if source.is_none() { source = Some(arg); } }
        }
    }

    let source = source.ok_or_else(|| anyhow::anyhow!("Missing SOURCE argument"))?;
    Ok((source, output))
}