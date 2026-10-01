# dprint-plugin-jupyter

[![](https://img.shields.io/crates/v/dprint-plugin-jupyter.svg)](https://crates.io/crates/dprint-plugin-jupyter) [![npm version](https://img.shields.io/npm/v/@dprint/jupyter.svg)](https://www.npmjs.com/package/@dprint/jupyter) [![CI](https://github.com/dprint/dprint-plugin-jupyter/workflows/CI/badge.svg)](https://github.com/dprint/dprint-plugin-jupyter/actions?query=workflow%3ACI)

Formats code blocks in Jupyter notebook files (`.ipynb`) using dprint plugins.

## Install

[Install](https://dprint.dev/install/) and [setup](https://dprint.dev/setup/) dprint.

Then in your project's directory with a dprint.json file, run:

```shellsession
dprint add jupyter
```

Then add some additional formatting plugins to format the code blocks with. For example:

```shellsession
dprint add typescript
dprint add markdown
dprint add ruff
```

Each code block is formatted with whichever plugin handles its language's file extension (ex. a `python` cell is formatted as a `.py` file and a `sql` cell as a `.sql` file). If you find a code block isn't being formatted with a plugin, please verify it's not a syntax error. After, open an [issue](https://github.com/dprint/dprint-plugin-jupyter/issues) about mapping that cell's language to the plugin's file extension (if you're interested in opening a PR, it's potentially an easy contribution).

## Configuration

Configuration is handled in other plugins.
