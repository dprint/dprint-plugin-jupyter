use super::configuration::Configuration;
use super::configuration::resolve_config;

use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::generate_plugin_code;
use dprint_core::plugins::CheckConfigUpdatesMessage;
use dprint_core::plugins::ConfigChange;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::FormatRange;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;
use dprint_core::plugins::SyncFormatRequest;
use dprint_core::plugins::SyncHostFormatRequest;
use dprint_core::plugins::SyncPluginHandler;

struct JupyterPluginHandler;

impl SyncPluginHandler<Configuration> for JupyterPluginHandler {
  fn resolve_config(
    &mut self,
    config: ConfigKeyMap,
    global_config: &GlobalConfiguration,
  ) -> PluginResolveConfigurationResult<Configuration> {
    let config = resolve_config(config, global_config);
    PluginResolveConfigurationResult {
      config: config.config,
      diagnostics: config.diagnostics,
      file_matching: FileMatchingInfo {
        file_extensions: vec!["ipynb".to_string()],
        file_names: vec![],
      },
    }
  }

  fn plugin_info(&mut self) -> PluginInfo {
    let version = env!("CARGO_PKG_VERSION").to_string();
    PluginInfo {
      name: env!("CARGO_PKG_NAME").to_string(),
      version: version.clone(),
      config_key: "jupyter".to_string(),
      help_url: "https://dprint.dev/plugins/jupyter".to_string(),
      config_schema_url: format!(
        "https://plugins.dprint.dev/dprint/dprint-plugin-jupyter/{}/schema.json",
        version
      ),
      update_url: Some("https://plugins.dprint.dev/dprint/dprint-plugin-jupyter/latest.json".to_string()),
    }
  }

  fn check_config_updates(&self, _message: CheckConfigUpdatesMessage) -> Result<Vec<ConfigChange>, anyhow::Error> {
    Ok(Vec::new())
  }

  fn license_text(&mut self) -> String {
    std::str::from_utf8(include_bytes!("../LICENSE")).unwrap().into()
  }

  fn format(
    &mut self,
    request: SyncFormatRequest<Configuration>,
    _format_with_host: impl FnMut(SyncHostFormatRequest) -> FormatResult,
  ) -> FormatResult {
    let file_text = String::from_utf8(request.file_bytes)?;
    super::format_text(&file_text, |path, text| {
      let additional_config = ConfigKeyMap::new();
      let request = SyncHostFormatRequest {
        file_path: &path,
        file_bytes: text.as_bytes(),
        range: FormatRange::None,
        override_config: &additional_config,
      };
      let maybe_bytes = format_with_host(request)?;
      match maybe_bytes {
        Some(bytes) => Ok(Some(String::from_utf8(bytes)?)),
        None => Ok(None),
      }
    })
    .map(|maybe_file_text| maybe_file_text.map(|file_text| file_text.into_bytes()))
  }
}

generate_plugin_code!(JupyterPluginHandler, JupyterPluginHandler);
