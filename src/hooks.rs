use anyhow::{Result, Context};
use mlua::{Lua, Value as LuaValue, Table};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Default)]
pub struct HookResult {
    pub updated_vars: Option<JsonValue>,
    pub created_files: Vec<(PathBuf, String)>,
}

fn load_hook_script(root: &Path, name: &str) -> Result<Option<String>> {
    let path = root.join("hooks").join(name);
    if !path.exists() { return Ok(None); }
    let s = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read hook script: {}", path.display()))?;
    Ok(Some(s))
}

fn json_to_lua_table(lua: &Lua, json: &JsonValue) -> Result<Table> {
    let table = lua.create_table()?;
    if let Some(obj) = json.as_object() {
        for (k, v) in obj.iter() {
            match v {
                JsonValue::String(s) => { table.set(k.as_str(), s.as_str())?; }
                JsonValue::Bool(b) => { table.set(k.as_str(), *b)?; }
                JsonValue::Number(n) => { 
                    if let Some(i) = n.as_i64() { table.set(k.as_str(), i)?; }
                    else if let Some(f) = n.as_f64() { table.set(k.as_str(), f)?; }
                }
                _ => {}
            }
        }
    }
    Ok(table)
}

fn lua_value_to_json(val: LuaValue) -> Option<JsonValue> {
    match val {
        LuaValue::Nil => None,
        LuaValue::Boolean(b) => Some(JsonValue::Bool(b)),
        LuaValue::Integer(i) => Some(JsonValue::Number(serde_json::Number::from(i))),
        LuaValue::Number(n) => serde_json::Number::from_f64(n).map(JsonValue::Number),
        LuaValue::String(s) => Some(JsonValue::String(s.to_str().ok()?.to_string())),
        LuaValue::Table(t) => {
            // Only handle simple object tables
            let mut obj = serde_json::Map::new();
            for (k, v) in t.pairs::<String, LuaValue>().flatten() {
                if let Some(j) = lua_value_to_json(v) { obj.insert(k, j); }
            }
            Some(JsonValue::Object(obj))
        }
        _ => None,
    }
}

fn run_hook(root: &Path, script_name: &str, vars: &JsonValue, ctx: &JsonValue) -> Result<HookResult> {
    let script = match load_hook_script(root, script_name)? { Some(s) => s, None => return Ok(HookResult::default()) };
    let lua = Lua::new();
    let globals = lua.globals();
    let vars_tbl = json_to_lua_table(&lua, vars)?;
    let ctx_tbl = json_to_lua_table(&lua, ctx)?;
    globals.set("vars", vars_tbl)?;
    globals.set("ctx", ctx_tbl)?;

    // Convention: script returns a table { vars = {..}, files = [{ path = "...", content = "..." }] }
    let val: LuaValue = lua.load(&script).eval()?;
    let mut result = HookResult::default();
    if let LuaValue::Table(t) = val {
        if let Ok(v) = t.get::<LuaValue>("vars") {
            result.updated_vars = lua_value_to_json(v);
        }
        if let Ok(files_tbl) = t.get::<Table>("files") {
            for item in files_tbl.sequence_values::<Table>().flatten() {
                let path: Option<String> = item.get("path").ok();
                let content: Option<String> = item.get("content").ok();
                if let (Some(p), Some(c)) = (path, content) {
                    result.created_files.push((PathBuf::from(p), c));
                }
            }
        }
    }
    Ok(result)
}

pub fn run_pre_prompt(root: &Path, current_vars: &JsonValue) -> Result<Option<JsonValue>> {
    let ctx = serde_json::json!({ "stage": "pre_prompt" });
    let res = run_hook(root, "pre_prompt.lua", current_vars, &ctx)?;
    Ok(res.updated_vars)
}

pub fn run_pre_gen(root: &Path, vars: &JsonValue, output: &Path) -> Result<HookResult> {
    let ctx = serde_json::json!({ "stage": "pre_gen_project", "output": output.to_string_lossy() });
    let res = run_hook(root, "pre_gen_project.lua", vars, &ctx)?;
    Ok(res)
}

pub fn run_post_gen(root: &Path, vars: &JsonValue, output: &Path) -> Result<HookResult> {
    let ctx = serde_json::json!({ "stage": "post_gen_project", "output": output.to_string_lossy() });
    let res = run_hook(root, "post_gen_project.lua", vars, &ctx)?;
    Ok(res)
}