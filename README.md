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

If the environment is not a TTY (e.g., during automated tests), Copilot falls back to reading answers from standard input line by line. Hit Enter to accept defaults.

## Template Manifest: `copilot.json`

Your template folder must include a `copilot.json` at its root. It defines variables and optional `_copy_without_render` paths.

Example:
```json
{
  "project_name": "hello_world",
  "author": "Alice",
  "private": false,
  "retries": 3,
  "license": ["MIT", "Apache-2.0", "GPL-3.0"],
  "_copy_without_render": ["tests/**"]
}
```

Notes:
- Variables whose names start with `_` are ignored for prompting.
- Strings, booleans, and numbers are prompted directly.
- Arrays are treated as a set of choices; the selected value is used during rendering.
- `_copy_without_render` entries are shell‑style glob patterns relative to the template root; matching files are copied as‑is.
  - Examples: `*.txt` (all text files), `tests/**` (everything under tests and its subdirectories), `hooks/**`.
  - Patterns use forward‑slash (`/`) separators and work consistently across OSes.
  - Empty strings are invalid and cause a manifest error.

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
└── {{ project_name }}/
    ├── base.txt
    └── child.txt
├── hooks/
│   ├── pre_prompt.lua
│   ├── pre_gen_project.lua
│   └── post_gen_project.lua
```

`child.txt` can include or import `base.txt` using the rendered path name.

## Development

Build:
```
cargo build
```

Run tests (includes an end‑to‑end interactive test that feeds stdin):
```
cargo test
```

## Migration Guide (from TOML manifest)
- Old `copilot.toml` is replaced by `copilot.json`.
- Regex validation and advanced TOML fields were removed to simplify the TUI.
- Non‑interactive `--vars` and `--non_interactive` flags have been removed; TUI is default, stdin fallback is used for CI.

## Notes
- If you reference templates by name in `include` or `import`, ensure the names match the rendered path (Copilot registers each file with its final rendered path).
- For Git sources, the repository is cloned to a temp directory and used as the template root.
- Git refs and submodules:
  - Use `#branch_or_tag` to checkout a specific ref, e.g. `https://example.com/repo.git#v1.2.0`.
  - Submodules are updated with `git submodule update --init --recursive` if `git` is available.
- `copilot.json` supports Jinja expressions in variable defaults. Examples:
  - `"name": "Demo App"`
  - `"slug": "{{ name | lower | replace(' ', '-') }}"`
  - Defaults are evaluated with dependency resolution before prompting; render errors fall back to original defaults.
- SVN/Mercurial support:
  - Template generation focuses on local templates to keep the binary small.
  - Submodule management uses system tools when available (no heavy libraries).

## Submodule Handling (Auto)

- Copilot automatically detects `.gitmodules` in the template source.
- If `git` is available and the source is a Git repository, Copilot runs:
  - `git submodule sync --recursive`
  - `git submodule update --init --recursive`
- Operations are best‑effort: failures are reported as warnings and generation continues.
- SVN externals: when `svn` is installed, externals are detected for visibility during status checks (no CLI required).
- This automatic preparation happens before copying the template into the temporary processing directory.

## Internationalization Note
This project’s documentation and CLI messages are provided in English for consistency across platforms.
- Hooks
  - Copilot auto-detects `hooks/` and executes Lua scripts if present.
  - `pre_prompt.lua`: runs before prompting. Return `{ vars = { ... } }` to override defaults.
  - `pre_gen_project.lua`: runs before rendering. Return `{ files = [{ path, content }, ...] }` to create files.
  - `post_gen_project.lua`: runs after rendering. Return `{ files = [...] }` to add post-generation artifacts.
  - Available globals in Lua: `vars` (table of current values), `ctx` (table with `stage`, `output`).
