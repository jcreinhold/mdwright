# Editor integration

mdwright ships a built-in language server. Point your editor at `mdwright lsp` and you get diagnostics, hover docs,
quick-fixes, and on-save / on-type formatting without configuring an external formatter command.

The smallest possible config is Helix:

```toml,no-check
[language-server.mdwright]
command = "mdwright"
args = ["lsp"]

[[language]]
name = "markdown"
language-servers = ["mdwright"]
```

## Position encoding gotcha

mdwright advertises UTF-8 position encoding. Clients that negotiate UTF-8 (VS Code 1.74+, Helix, Zed, neovim 0.10+)
get the full surface: diagnostics, hover, formatting, range formatting, on-type formatting, and code actions. Clients
that only support UTF-16 get diagnostics + hover; formatting and code-action providers are withdrawn rather than risk
corrupting non-ASCII sources, and a warning is logged via `window/logMessage`. Check your editor's LSP log if formatting
unexpectedly does nothing, it usually means the client never granted UTF-8.

## VS Code

There is no official extension yet; this section will be replaced when one ships. Until then, install any generic-LSP
client extension (e.g. [`vscode-glspc`](https://marketplace.visualstudio.com/items?itemName=jdinhlife.gruvbox)) and
point it at:

```jsonc,no-check
{
  "[markdown]": {
    "editor.defaultFormatter": "<your-lsp-client-extension-id>"
  },
  "yourLspClient.servers": [
    {
      "command": "mdwright",
      "args": ["lsp"],
      "languages": ["markdown"]
    }
  ]
}
```

## Helix

Add to `~/.config/helix/languages.toml` (or the workspace `.helix/languages.toml`):

```toml,no-check
[language-server.mdwright]
command = "mdwright"
args = ["lsp"]

[[language]]
name = "markdown"
language-servers = ["mdwright"]
auto-format = true
```

`:lsp-restart` after editing. Helix's `space-a` opens the code-action menu; pick ``Fix `bare-url`: …`` for a single
diagnostic or `Apply all mdwright safe fixes` to run every safe quick-fix at once.

## Zed

Add to `~/.config/zed/settings.json`:

```jsonc,no-check
{
  "lsp": {
    "mdwright": {
      "binary": {
        "path": "mdwright",
        "arguments": ["lsp"]
      }
    }
  },
  "languages": {
    "Markdown": {
      "language_servers": ["mdwright"],
      "format_on_save": "on"
    }
  }
}
```

## Neovim

Using `nvim-lspconfig` on neovim 0.10+:

```lua,no-check
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

if not configs.mdwright then
  configs.mdwright = {
    default_config = {
      cmd = { "mdwright", "lsp" },
      filetypes = { "markdown" },
      root_dir = lspconfig.util.find_git_ancestor,
      settings = {},
    },
  }
end

lspconfig.mdwright.setup({
  on_attach = function(_, bufnr)
    vim.api.nvim_create_autocmd("BufWritePre", {
      buffer = bufnr,
      callback = function() vim.lsp.buf.format({ async = false }) end,
    })
  end,
})
```

## Configuration

The server discovers `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml`'s `[tool.mdwright]` table by walking up from
the workspace root, exactly like the CLI. Edit one of those files and the server re-lints every open buffer on the next
file-watcher event; `workspace/didChangeConfiguration` triggers the same refresh.

## Range-formatting caveats

`textDocument/rangeFormatting` and `textDocument/onTypeFormatting` snap the
requested range out to the nearest whole top-level block before formatting (see
[`format_range`'s rustdoc](https://docs.rs/mdwright/latest/mdwright/fn.format_range.html)). For sources without
document-scope reorderable constructs the snapped output is a verbatim substring of the whole-document format; link
definitions (`[label]: dest`) and footnote definitions (`[^label]: …`) are document-scope, so a range format may leave
them in place where a whole-document format would have moved them to the canonical location. Save the file (or invoke
whole-document formatting) periodically to reconcile.

## See also

- [Pre-commit](pre-commit.md): backstop for missed editor saves.
- [Lint vs. format](../concepts/lint-vs-format.md): editor flow drives both pipelines.
