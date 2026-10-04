use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use dprint_core::configuration::*;
use dprint_development::*;
use dprint_plugin_jupyter::configuration::resolve_config;
use dprint_plugin_jupyter::*;

#[test]
fn test_specs() {
  let global_config = GlobalConfiguration::default();

  run_specs(
    &PathBuf::from("./tests/specs"),
    &ParseSpecOptions {
      default_file_name: "file.py",
    },
    &RunSpecsOptions {
      fix_failures: false,
      format_twice: true,
    },
    {
      let global_config = global_config.clone();
      Arc::new(move |_file_path, file_text, range, spec_config| {
        let spec_config: ConfigKeyMap = serde_json::from_value(spec_config.clone().into()).unwrap();
        let config_result = resolve_config(spec_config, &global_config);
        ensure_no_diagnostics(&config_result.diagnostics);

        let format_cell_text = |path: &Path, text: String| {
          if path.ends_with("code_block.py") {
            if !text.ends_with("_python") {
              Ok(Some(format!("{}_python", text)))
            } else {
              Ok(None)
            }
          } else if path.ends_with("code_block.md") {
            if !text.ends_with("_markdown") {
              Ok(Some(format!("{}_markdown", text)))
            } else {
              Ok(None)
            }
          } else if path.ends_with("code_block.ts") {
            if !text.ends_with("_typescript") {
              Ok(Some(format!("{}_typescript", text)))
            } else {
              Ok(None)
            }
          } else if path.ends_with("code_block.tsx") {
            if !text.ends_with("_tsx") {
              Ok(Some(format!("{}_tsx", text)))
            } else {
              Ok(None)
            }
          } else if path.ends_with("code_block.multiline") {
            if !text.ends_with("_multiline") {
              Ok(Some(format!("{}\n_multiline\n", text)))
            } else {
              Ok(None)
            }
          } else if path.ends_with("code_block.error") {
            Err("failed formatting".into())
          } else if path.ends_with("code_block.sql") {
            if !text.ends_with("_sql") {
              Ok(Some(format!("{}_sql", text)))
            } else {
              Ok(None)
            }
          } else {
            Ok(None)
          }
        };
        match range {
          Some(range) => format_text_range(file_text, range, format_cell_text),
          None => format_text(file_text, format_cell_text),
        }
        .map_err(Into::into)
      })
    },
    Arc::new(move |_file_path, _file_text, _spec_config| panic!("Plugin does not support dprint-core tracing.")),
  )
}
