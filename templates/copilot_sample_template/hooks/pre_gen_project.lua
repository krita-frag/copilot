-- pre_gen_project.lua
-- Goal: generate files before template rendering based on current vars.
-- The script can access `vars` (table) and `ctx` (table with `stage` and `output`).

local name = vars.project_name or "project"
local author = vars.author or "Unknown"
local license = vars.license or "MIT"
local slug = vars.project_slug or name

local config_json = [[{
  "name": "%s",
  "slug": "%s",
  "author": "%s",
  "license": "%s",
  "generated": "pre"
}]]

return {
  files = {
    { path = "hook_pre.txt", content = "Pre-generation hook has run (stage=" .. tostring(ctx.stage) .. ")" },
    { path = "bootstrap/config.json", content = string.format(config_json, name, slug, author, license) },
    { path = "LICENSE.txt", content = "License: " .. license }
  }
}