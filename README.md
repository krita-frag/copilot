# Copilot

Copilot is a cross‑platform project scaffolding CLI similar to Cookiecutter, now with a TUI‑based interactive flow and a JSON manifest.

## Key Features
- TUI interactive prompts that adapt to variable types: text, boolean, number, and choices.
- Cookiecutter‑style replacements using MiniJinja for both file content and file/folder names.
- Template manifest in `copilot.json` (similar to `cookiecutter.json`).
- Supports `_copy_without_render` to skip Jinja rendering using shell-style glob patterns.
- Lua hooks system: detect `hooks/` and run `pre_prompt.lua`, `pre_gen_project.lua`, `post_gen_project.lua`.
- Works with local template folders and Git repositories.

## Install
```
cargo install --path .
```

## Usage

Basic usage with a local template folder:
```
copilot <source> --output <dir>
```
- `source`: Path to a local template directory, or a Git URL.
- `--output`: Destination directory. Defaults to the current directory.

Examples:
- Local template: `copilot templates/copilot_sample_template --output ./out`
- Current directory output: `copilot templates/copilot_sample_template`
- Git template: `copilot https://github.com/<user>/<repo>.git --output ./out`

When run in a terminal, the tool presents a TUI and automatically chooses the best input component for each variable:
- String → text input
- Boolean → checkbox (y/n)
- Number → numeric input
- Array → choices (dropdown/select)
- Object → nested JSON input (recursively prompted)

If the environment is not a TTY (e.g., during automated tests), Copilot falls back to reading answers from standard input line by line. Hit Enter to accept defaults.

## Template Manifest: `copilot.json`

Your template folder must include a `copilot.json` at its root. It defines variables and optional `_copy_without_render` paths.

Example:
```json
{
  "project_name": "Hello World",
  "project_slug": "hello_world",
  "author": "Alice",
  "private": false,
  "retries": 3,
  "license": ["MIT", "Apache-2.0", "GPL-3.0"],
  "_copy_without_render": ["tests/**"]
}
```

## Rendering Rules
- Both file contents and path segments are rendered with MiniJinja.
- Registered templates support `include`/`import` across the template folder.
- Paths matching `_copy_without_render` glob patterns are copied without Jinja rendering.

## Example Template Structure
```
templates/copilot_sample_template/
├── copilot.json
├── README.md
├── hooks/
└── {{ project_slug }}/
├── hooks/
│   ├── pre_prompt.lua
│   ├── pre_gen_project.lua
│   └── post_gen_project.lua
```

## Development

Build:
```
cargo build
```

Run tests (includes an end‑to‑end interactive test that feeds stdin):
```
cargo test
```

## Internationalization Note
This project’s documentation and CLI messages are provided in English for consistency across platforms.
- Hooks
  - Copilot auto-detects `hooks/` and executes Lua scripts if present.
  - `pre_prompt.lua`: runs before prompting. Return `{ vars = { ... } }` to override defaults.
  - `pre_gen_project.lua`: runs before rendering. Return `{ files = [{ path, content }, ...] }` to create files.
  - `post_gen_project.lua`: runs after rendering. Return `{ files = [...] }` to add post-generation artifacts.
  - Available globals in Lua: `vars` (table of current values), `ctx` (table with `stage`, `output`).
