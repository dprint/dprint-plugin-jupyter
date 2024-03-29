use super::configuration::{resolve_config, Configuration};

use dprint_core::configuration::{ConfigKeyMap, GlobalConfiguration, ResolveConfigurationResult};
use dprint_core::generate_plugin_code;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::SyncPluginHandler;
use dprint_core::plugins::SyncPluginInfo;
use std::path::Path;

struct JupyterPluginHandler;

impl SyncPluginHandler<Configuration> for JupyterPluginHandler {
  fn resolve_config(
    &mut self,
    config: ConfigKeyMap,
    global_config: &GlobalConfiguration,
  ) -> ResolveConfigurationResult<Configuration> {
    resolve_config(config, global_config)
  }

  fn plugin_info(&mut self) -> SyncPluginInfo {
    let version = env!("CARGO_PKG_VERSION").to_string();
    SyncPluginInfo {
      info: PluginInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: version.clone(),
        config_key: "jupyter".to_string(),
        help_url: "https://dprint.dev/plugins/jupyter".to_string(),
        config_schema_url: format!(
          "https://plugins.dprint.dev/dprint/dprint-plugin-jupyter/{}/schema.json",
          version
        ),
        update_url: Some("https://plugins.dprint.dev/dprint/dprint-plugin-jupyter/latest.json".to_string()),
      },
      file_matching: FileMatchingInfo {
        file_extensions: vec!["ipynb".to_string()],
        file_names: vec![],
      },
    }
  }

  fn license_text(&mut self) -> String {
    std::str::from_utf8(include_bytes!("../LICENSE")).unwrap().into()
  }

  fn format(
    &mut self,
    _file_path: &Path,
    file_bytes: Vec<u8>,
    _config: &Configuration,
    mut format_with_host: impl FnMut(&Path, Vec<u8>, &ConfigKeyMap) -> FormatResult,
  ) -> FormatResult {
    let file_text = String::from_utf8(file_bytes)?;
    super::format_text(&file_text, |path, text| {
      let maybe_bytes = format_with_host(path, text.into_bytes(), &ConfigKeyMap::new())?;
      match maybe_bytes {
        Some(bytes) => Ok(Some(String::from_utf8(bytes)?)),
        None => Ok(None),
      }
    })
    .map(|maybe_file_text| maybe_file_text.map(|file_text| file_text.into_bytes()))
  }
}

generate_plugin_code!(JupyterPluginHandler, JupyterPluginHandler);
