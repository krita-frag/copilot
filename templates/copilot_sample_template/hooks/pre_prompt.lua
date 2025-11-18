-- pre_prompt.lua
-- Goal: adjust default variables before interactive prompts.
-- You can compute derived values or change defaults based on existing ones.

local function slugify(s)
  s = string.lower(s or "")
  s = s:gsub("%s+", "-")       -- spaces -> '-'
  s = s:gsub("_+", "-")        -- underscores -> '-'
  s = s:gsub("[^a-z0-9%-]", "") -- strip non-alnum except '-'
  s = s:gsub("%-+", "-")        -- collapse multiple '-'
  return s
end

local name = vars.project_name or "project"
local author = vars.author or "Unknown"
local is_private = vars.private == true
local retries = tonumber(vars.retries or 1) or 1

-- Choose a default license based on privacy
local default_license = is_private and "GPL-3.0" or "MIT"

return {
  vars = {
    project_slug = slugify(name),
    readme_title = name .. " by " .. author,
    license = default_license,
    retries = math.max(1, retries),
  }
}