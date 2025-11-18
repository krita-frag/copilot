use anyhow::{Context, Result};
// Custom minimal glob matcher to avoid heavy regex dependencies
use serde_json::Value;
use std::collections::BTreeMap;
use minijinja::Environment;
use std::{fs, path::Path};

#[derive(Debug, Clone)]
pub enum VarKind {
    String,
    Bool,
    Number,
    Choice(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct VarDef {
    pub name: String,
    pub kind: VarKind,
    pub default: Option<Value>,
    // Optional labels for choices when the variable is defined as a dictionary.
    // Keys are the actual values; values are human-friendly labels.
    pub choice_labels: Option<BTreeMap<String, String>>, // None for non-choice vars
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub variables: Vec<VarDef>,
    pub copy_without_render: Vec<String>,
}

pub fn load_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join("copilot.json");
    if !path.exists() {
        return Ok(Manifest::default());
    }
    let s = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read template manifest: {}", path.display()))?;
    let root: Value = serde_json::from_str(&s)
        .with_context(|| "Failed to parse template manifest copilot.json (JSON)")?;

    let mut manifest = Manifest::default();
    let obj = root.as_object().ok_or_else(|| anyhow::anyhow!("copilot.json root must be a JSON object"))?;

    // Built-in fields
    if let Some(Value::Array(arr)) = obj.get("_copy_without_render") {
        let mut paths = Vec::new();
        for v in arr {
            if let Some(s) = v.as_str() {
                paths.push(s.to_string());
            }
        }
        manifest.copy_without_render = paths;
    }

    // `_extensions` is intentionally ignored to keep the manifest minimal.
    // Users can implement custom logic via hooks instead.

    // Note: We intentionally do NOT parse `__prompts__` to keep the code concise
    // and avoid hardcoding prompt-related structures. Choices can be expressed
    // directly via dictionary format in variable definitions (see below).

    // Variable definitions
    for (k, v) in obj.iter() {
        if k.starts_with('_') { continue; }
        let def = match v {
            Value::String(_) => VarDef { name: k.clone(), kind: VarKind::String, default: Some(v.clone()), choice_labels: None },
            Value::Bool(_) => VarDef { name: k.clone(), kind: VarKind::Bool, default: Some(v.clone()), choice_labels: None },
            Value::Number(_) => VarDef { name: k.clone(), kind: VarKind::Number, default: Some(v.clone()), choice_labels: None },
            Value::Array(arr) => {
                // Only support an array of string choices
                let choices: Vec<String> = arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
                if choices.is_empty() {
                    continue; // skip unsupported types
                }
                let default = choices.first().map(|s| Value::String(s.clone()));
                VarDef { name: k.clone(), kind: VarKind::Choice(choices), default, choice_labels: None }
            }
            Value::Object(map) => {
                // Dictionary-format choices support (avoids __prompts__ mechanism):
                // Treat object keys as allowed choice values, ignore special key "__prompt__".
                // Example: { "__prompt__": "Choose...", "pytest": "pytest", "unittest": "unittest" }
                let mut keys: Vec<String> = Vec::new();
                let mut labels: BTreeMap<String, String> = BTreeMap::new();
                for (kk, vv) in map.iter() {
                    if kk == "__prompt__" { continue; }
                    if vv.is_string() {
                        keys.push(kk.to_string());
                        labels.insert(kk.to_string(), vv.as_str().unwrap_or(kk).to_string());
                    }
                }
                if keys.is_empty() { continue; }
                let default = keys.first().map(|s| Value::String(s.clone()));
                VarDef { name: k.clone(), kind: VarKind::Choice(keys), default, choice_labels: Some(labels) }
            }
            _ => continue,
        };
        manifest.variables.push(def);
    }
    Ok(manifest)
}

impl Manifest {
    // Compile copy filter using minimal glob support.
    // Supported:
    // - Segment wildcard '*'
    // - Recursive wildcard '**' across directory boundaries
    // Invalid characters like '[' or ']' will produce an error to match tests.
    pub fn compile_copy_filter(&self) -> Result<CopyFilter> {
        let mut pats = Vec::new();
        for pat in &self.copy_without_render {
            let p = pat.trim();
            if p.is_empty() { anyhow::bail!("Invalid empty pattern in _copy_without_render"); }
            if p.contains('[') || p.contains(']') {
                anyhow::bail!(format!("Invalid glob pattern: {}", pat));
            }
            pats.push(p.replace('\\', "/"));
        }
        Ok(CopyFilter { patterns: pats })
    }

    // Evaluate variable default values using Jinja syntax with dependency resolution.
    // Simplified version: no hardcoded filters, keeping implementation concise.
    // - Supports string defaults like "{{ project_name }}-service"
    // - Performs multiple passes until values stabilize or max iteration threshold is reached
    // - On render errors, keeps original default to preserve backward compatibility
    pub fn evaluate_defaults(&self, initial: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>> {
        let env = Environment::new();
        let mut vars = initial.clone();
        let max_passes = self.variables.len().max(1) * 2;
        for _ in 0..max_passes {
            let mut changed = false;
            for def in &self.variables {
                if let Some(Value::String(s)) = def.default.as_ref() {
                    match env.render_str(s, &vars) {
                        Ok(rendered) => {
                            let new_val = Value::String(rendered);
                            if vars.get(&def.name) != Some(&new_val) {
                                vars.insert(def.name.clone(), new_val);
                                changed = true;
                            }
                        }
                        Err(_) => {
                            if let Some(orig) = def.default.clone() {
                                if vars.get(&def.name) != Some(&orig) {
                                    vars.insert(def.name.clone(), orig);
                                    changed = true;
                                }
                            }
                        }
                    }
                } else if let Some(orig) = def.default.clone() {
                    if vars.get(&def.name) != Some(&orig) {
                        vars.insert(def.name.clone(), orig);
                        changed = true;
                    }
                }
            }
            if !changed { break; }
        }
        Ok(vars)
    }
}

#[derive(Debug, Clone)]
pub struct CopyFilter {
    patterns: Vec<String>,
}

impl CopyFilter {
    pub fn is_match(&self, rel: &str) -> bool {
        let text = rel.replace('\\', "/");
        for pat in &self.patterns {
            if pattern_matches(pat, &text) { return true; }
        }
        false
    }
}

fn segment_matches(pat: &str, s: &str) -> bool {
    if !pat.contains('*') { return pat == s; }
    // Simple wildcard matcher: '*' matches any sequence within segment
    let mut si = 0usize;
    let mut first = true;
    for token in pat.split('*') {
        if token.is_empty() { if first { /* leading '*' */ } else { /* consecutive '*' */ } }
        else if first && !pat.starts_with('*') {
            if !s[si..].starts_with(token) { return false; }
            si += token.len();
        } else {
            // find token anywhere after si
            if let Some(pos_rel) = s[si..].find(token) {
                si += pos_rel + token.len();
            } else { return false; }
        }
        first = false;
    }
    if !pat.ends_with('*') { si == s.len() } else { true }
}

fn pattern_matches(pat: &str, path: &str) -> bool {
    let psegs: Vec<&str> = pat.split('/').collect();
    let ssegs: Vec<&str> = path.split('/').collect();
    fn rec(p: &[&str], s: &[&str]) -> bool {
        if p.is_empty() { return s.is_empty(); }
        if p[0] == "**" {
            if p.len() == 1 { return true; } // matches the rest
            for skip in 0..=s.len() {
                if rec(&p[1..], &s[skip..]) { return true; }
            }
            return false;
        }
        if s.is_empty() { return false; }
        if segment_matches(p[0], s[0]) { return rec(&p[1..], &s[1..]); }
        false
    }
    rec(&psegs, &ssegs)
}