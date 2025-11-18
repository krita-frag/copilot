-- post_gen_project.lua
-- Goal: generate files after template rendering; often used for summaries.

local name = vars.project_name or "project"
local slug = vars.project_slug or name
local retries = tonumber(vars.retries or 1) or 1

local summary = [[# Generation Summary

Project: %s
Slug:    %s
Retries: %d
Output:  %s

This file was created by post-gen hook after rendering.
]]

return {
  files = {
    { path = "hook_post.txt", content = "Post-generation hook has run (stage=" .. tostring(ctx.stage) .. ")" },
    { path = "POST.md", content = string.format(summary, name, slug, retries, tostring(ctx.output)) }
  }
}