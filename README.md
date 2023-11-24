# dprint-plugin-jupyter

[![](https://img.shields.io/crates/v/dprint-plugin-jupyter.svg)](https://crates.io/crates/dprint-plugin-jupyter) [![CI](https://github.com/dprint/dprint-plugin-jupyter/workflows/CI/badge.svg)](https://github.com/dprint/dprint-plugin-jupyter/actions?query=workflow%3ACI)

Formats code blocks in Jupyter notebook files (`.ipynb`) using dprint plugins.

## Install

[Install](https://dprint.dev/install/) and [setup](https://dprint.dev/setup/) dprint.

Then in your project's directory with a dprint.json file, run:

```shellsession
dprint config add jupyter
```

Then add some additional formatting plugins to format the code blocks with. For example:

```shellsession
dprint config add typescript
dprint config add markdown
dprint config add ruff
```

If you find a code block isn't being formatted with a plugin, please verify it's not a syntax error. After, open an [issue](https://github.com/dprint/dprint-plugin-jupyter/issues) about adding support for that plugin (if you're interested in opening a PR, it's potentially an easy contribution).

## Configuration

Configuration is handled in other plugins.
