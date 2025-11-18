use anyhow::{Result, Context, anyhow};
use std::{path::{Path, PathBuf}, fs};
use walkdir::WalkDir;
use minijinja::Environment;
use serde::Serialize;
use serde_json::to_value as to_json_value;
use crate::manifest::CopyFilter;
use crate::util::{sanitize_slug_python, is_safe_path_segment};

pub fn render_all<T: Serialize>(template_dir: &Path, output_dir: &Path, vars: &T, copy_filter: &CopyFilter) -> Result<()> {
    let mut env = Environment::new();
    // Normalize and enforce Python-importable project_slug in vars
    let mut vars_json = to_json_value(vars).with_context(|| "Failed to serialize template variables")?;
    // Acquire project_slug with graceful fallback from project_title/project_name
    let slug_source: String = match vars_json.get("project_slug").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            let fallback = vars_json
                .get("project_title").and_then(|v| v.as_str())
                .or_else(|| vars_json.get("project_name").and_then(|v| v.as_str()))
                .unwrap_or("project");
            sanitize_slug_python(fallback)
        }
    };
    let normalized_slug = sanitize_slug_python(&slug_source);
    if normalized_slug.is_empty() {
        anyhow::bail!("Invalid 'project_slug' after normalization: empty");
    }
    if let Some(map) = vars_json.as_object_mut() {
        map.insert("project_slug".to_string(), serde_json::Value::String(normalized_slug.clone()));
    }

    // Detect the single main project directory that contains Jinja variables for {{ project_slug }}
    let mut main_dir_tpl: Option<String> = None;
    for entry in fs::read_dir(template_dir).with_context(|| format!("Failed to read directory: {}", template_dir.display()))? {
        let entry = entry.with_context(|| "Failed to iterate template root")?;
        let md = entry.metadata().with_context(|| "Failed to read entry metadata")?;
        if !md.is_dir() { continue; }
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if name_str.contains("{{") && name_str.contains("}}") && name_str.contains("project_slug") {
            if main_dir_tpl.is_some() {
                anyhow::bail!("Multiple main project directories detected. Only one '{{ project_slug }}' directory is supported.");
            }
            main_dir_tpl = Some(name_str);
        }
    }
    let main_dir_tpl = main_dir_tpl.ok_or_else(|| anyhow!("Main project directory using '{{ project_slug }}' not found at template root"))?;
    // Pass 1: collect templates and register into environment (to support extends/include/import)
    struct Item { name: String, rel: PathBuf, src_path: PathBuf, copy_raw: bool }
    struct PendingTpl { name: String, content: String, has_extends: bool }
    let mut items: Vec<Item> = Vec::new();
    let mut to_register: Vec<PendingTpl> = Vec::new();
    for entry in WalkDir::new(template_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() { continue; }
        let rel = path
            .strip_prefix(template_dir)
            .with_context(|| format!("Failed to compute relative path: {}", path.display()))?;
        if rel.to_string_lossy() == "copilot.json" { continue; }
        if rel.components().any(|c| c.as_os_str() == ".git") { continue; }
        // Ignore root-level hooks/ folder (used only for execution)
        if let Some(first) = rel.components().next() {
            if first.as_os_str() == "hooks" { continue; }
        }
        // Filter: only process files under the main project directory (single main project)
        if let Some(first) = rel.components().next() {
            let first_str = first.as_os_str().to_string_lossy();
            if first_str != main_dir_tpl {
                // Skip any content not under the main project dir
                continue;
            }
        }

        // Render each segment of the relative path to get the final name
        let mut rendered_rel = PathBuf::new();
        for comp in rel.components() {
            let comp_str = comp.as_os_str().to_string_lossy();
            let out_segment = env.render_str(&comp_str, &vars_json)
                .with_context(|| format!("Failed to render path segment: {}", comp_str))?;
            if !is_safe_path_segment(&out_segment) {
                anyhow::bail!(format!("Unsafe rendered path segment: {}", out_segment));
            }
            rendered_rel.push(out_segment);
        }
        let name_owned = rendered_rel.to_string_lossy().replace('\\', "/");
        // Apply _copy_without_render patterns relative to the main project directory.
        // Example: pattern "tests/**" should match "{{ project_slug }}/tests/**" paths.
        let mut comps = rel.components();
        let _first = comps.next(); // strip the main project directory component
        let inner_rel: std::path::PathBuf = comps.collect();
        let inner_rel_str = inner_rel.to_string_lossy().replace('\\', "/");
        let copy_raw = copy_filter.is_match(&inner_rel_str);

        if !copy_raw {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read template file: {}", path.display()))?;
            let has_extends = content.contains("{% extends");
            to_register.push(PendingTpl { name: name_owned.clone(), content, has_extends });
        }
        items.push(Item { name: name_owned, rel: rendered_rel, src_path: path.to_path_buf(), copy_raw });
    }

    // Register templates: first those without extends (likely bases), then those with extends
    to_register.sort_by_key(|t| t.has_extends);
    for t in to_register {
        // Leak to 'static to satisfy MiniJinja environment lifetimes
        let leaked_name: &'static str = Box::leak(t.name.into_boxed_str());
        let leaked_content: &'static str = Box::leak(t.content.into_boxed_str());
        env.add_template(leaked_name, leaked_content)
            .with_context(|| format!("Failed to add template: {}", leaked_name))?;
    }

    // Templates are registered in the environment

    // Ensure output root exists and cache canonical root for performance
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output root: {}", output_dir.display()))?;
    let output_canon = output_dir.canonicalize()
        .with_context(|| format!("Failed to canonicalize output root: {}", output_dir.display()))?;

    // Pass 2: render or copy into the destination using the final names
    for item in items {
        let target_path = crate::util::safe_resolve_under_canon(&output_canon, &item.rel)?;
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        if item.copy_raw {
            let bytes = fs::read(&item.src_path)
                .with_context(|| format!("Failed to read template file: {}", item.src_path.display()))?;
            fs::write(&target_path, bytes)
                .with_context(|| format!("Failed to write file: {}", target_path.display()))?;
        } else {
            let tpl = env.get_template(&item.name)
                .with_context(|| format!("Failed to get template: {}", item.name))?;
            let rendered = tpl.render(&vars_json)
                .with_context(|| format!("Failed to render file: {}", item.src_path.display()))?;
            fs::write(&target_path, rendered)
                .with_context(|| format!("Failed to write file: {}", target_path.display()))?;
        }
    }
    Ok(())
}

// slug normalization moved to util::sanitize_slug_python