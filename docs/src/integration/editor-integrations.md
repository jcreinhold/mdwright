# Editor integrations

> **Status:** First-class LSP support and editor extensions are tracked under prompt 29. Until
> then, run mdwright on save via your editor's generic external-command hook.

## VS Code

Add to `.vscode/settings.json`:

```json,no-check
{
  "editor.formatOnSave": true,
  "[markdown]": {
    "editor.defaultFormatter": "esbenp.prettier-vscode"
  }
}
```

…then replace Prettier with a task that shells out to `mdwright fmt`. The `Run on Save` extension
(`emeraldwalk.runonsave`) is one path:

```json,no-check
{
  "emeraldwalk.runonsave": {
    "commands": [
      {
        "match": "\\.md$",
        "cmd": "mdwright fmt \"${file}\""
      }
    ]
  }
}
```

## Neovim

Using [conform.nvim](https://github.com/stevearc/conform.nvim):

```lua,no-check
require("conform").setup({
  formatters_by_ft = {
    markdown = { "mdwright" },
  },
  formatters = {
    mdwright = {
      command = "mdwright",
      args = { "fmt", "-" },
      stdin = true,
    },
  },
})
```

mdwright reads stdin when no path arguments are passed and writes the formatted output to stdout.

## Helix

`languages.toml`:

```toml,no-check
[[language]]
name = "markdown"
formatter = { command = "mdwright", args = ["fmt", "-"] }
auto-format = true
```

## See also

- [Pre-commit](pre-commit.md) — backstop for missed editor saves.
- [Lint vs. format](../concepts/lint-vs-format.md) — editor flow only invokes the formatter.
