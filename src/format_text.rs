use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

use crate::text_changes::apply_text_changes;
use crate::text_changes::TextChange;
use anyhow::Result;
use jsonc_parser::CollectOptions;
use jsonc_parser::ParseOptions;

pub fn format_text(
  input_text: &str,
  format_with_host: impl FnMut(&Path, String) -> Result<Option<String>>,
) -> Result<Option<String>> {
  let parse_result = jsonc_parser::parse_to_ast(
    input_text,
    &CollectOptions {
      comments: false,
      tokens: false,
    },
    &ParseOptions {
      allow_comments: true,
      allow_loose_object_property_names: true,
      allow_trailing_commas: true,
    },
  )?;
  let Some(root_value) = parse_result.value else {
    return Ok(None);
  };

  Ok(format_root(input_text, &root_value, format_with_host))
}

fn format_root(
  input_text: &str,
  root_value: &jsonc_parser::ast::Value,
  mut format_with_host: impl FnMut(&Path, String) -> Result<Option<String>>,
) -> Option<String> {
  let root_obj = root_value.as_object()?;
  let maybe_default_language = get_metadata_language(root_obj);
  let cells = root_value.as_object()?.get_array("cells")?;

  let text_changes: Vec<TextChange> = cells
    .elements
    .iter()
    .filter_map(|element| get_cell_text_change(input_text, element, maybe_default_language, &mut format_with_host))
    .collect();

  if text_changes.is_empty() {
    None
  } else {
    Some(apply_text_changes(input_text, text_changes))
  }
}

fn get_cell_text_change(
  file_text: &str,
  cell: &jsonc_parser::ast::Value,
  maybe_default_language: Option<&str>,
  format_with_host: &mut impl FnMut(&Path, String) -> Result<Option<String>>,
) -> Option<TextChange> {
  let cell = cell.as_object()?;
  let cell_language = get_cell_vscode_language_id(cell).or_else(|| {
    let cell_type = cell.get_string("cell_type")?;
    match cell_type.value.as_ref() {
      "markdown" => Some("markdown"),
      "code" => maybe_default_language,
      _ => None,
    }
  })?;
  let code_block = analyze_code_block(cell, file_text)?;
  let file_path = language_to_path(cell_language)?;
  let formatted_text = format_with_host(&file_path, code_block.source).ok()??;
  // many plugins will add a final newline, but that doesn't look nice in notebooks, so trim it off
  let formatted_text = formatted_text.trim_end();

  let new_text = build_json_text(formatted_text, code_block.indent_text);

  Some(TextChange {
    range: code_block.replace_range,
    new_text,
  })
}

struct CodeBlockText<'a> {
  indent_text: &'a str,
  replace_range: std::ops::Range<usize>,
  source: String,
}

fn analyze_code_block<'a>(cell: &jsonc_parser::ast::Object<'a>, file_text: &'a str) -> Option<CodeBlockText<'a>> {
  let mut indent_text = "";
  let mut replace_range = std::ops::Range::default();
  let cell_source = match &cell.get("source")?.value {
    jsonc_parser::ast::Value::Array(items) => {
      let mut strings = Vec::with_capacity(items.elements.len());
      for (i, element) in items.elements.iter().enumerate() {
        let string_lit = element.as_string_lit()?;
        if i == 0 {
          indent_text = get_indent_text(file_text, string_lit.range.start);
          replace_range.start = string_lit.range.start;
        }
        if i == items.elements.len() - 1 {
          replace_range.end = string_lit.range.end;
        }
        strings.push(&string_lit.value);
      }

      let mut text = String::with_capacity(strings.iter().map(|s| s.len()).sum::<usize>());
      for string in strings {
        text.push_str(string);
      }
      text
    }
    jsonc_parser::ast::Value::StringLit(string) => string.value.to_string(),
    _ => return None,
  };
  Some(CodeBlockText {
    indent_text,
    replace_range,
    source: cell_source,
  })
}

/// Turn the formatted text into a json array, split up by line breaks.
fn build_json_text(formatted_text: &str, indent_text: &str) -> String {
  let mut new_text = String::new();
  let mut current_end_index = 0;
  for (i, line) in formatted_text.split('\n').enumerate() {
    current_end_index += line.len();
    if i > 0 {
      new_text.push_str(",\n");
      new_text.push_str(indent_text);
    }
    let is_last_line = current_end_index == formatted_text.len();
    new_text.push_str(
      &serde_json::to_string(
        if is_last_line {
          Cow::Borrowed(line)
        } else {
          Cow::Owned(format!("{}\n", line))
        }
        .as_ref(),
      )
      .unwrap(),
    );
    current_end_index += 1;
  }
  new_text
}

fn get_metadata_language<'a>(root_obj: &'a jsonc_parser::ast::Object<'a>) -> Option<&'a str> {
  let language_info = root_obj.get_object("metadata")?.get_object("language_info")?;
  Some(&language_info.get_string("name")?.value)
}

fn get_cell_vscode_language_id<'a>(cell: &'a jsonc_parser::ast::Object<'a>) -> Option<&'a str> {
  let cell_metadata = cell.get_object("metadata")?;
  let cell_language_info = cell_metadata.get_object("vscode")?;
  Some(&cell_language_info.get_string("languageId")?.value)
}

fn language_to_path(language: &str) -> Option<PathBuf> {
  let ext = match language.to_lowercase().as_str() {
    "bash" => Some("sh"),
    "c++" => Some("cpp"),
    "css" => Some("css"),
    "csharp" => Some("cs"),
    "html" => Some("html"),
    "go" => Some("go"),
    "kotlin" => Some("kt"),
    "json" => Some("json"),
    "julia" => Some("jl"),
    "markdown" => Some("md"),
    "typescript" => Some("ts"),
    "javascript" => Some("js"),
    "perl" => Some("perl"),
    "php" => Some("php"),
    "python" | "python3" => Some("py"),
    "r" => Some("r"),
    "ruby" => Some("rb"),
    "scala" => Some("scala"),
    "sql" => Some("sql"),
    "yaml" => Some("yml"),
    _ => None,
  };
  ext.map(|ext| PathBuf::from(format!("code_block.{}", ext)))
}

fn get_indent_text(file_text: &str, start_pos: usize) -> &str {
  let preceeding_text = &file_text[..start_pos];
  let whitespace_start = preceeding_text.trim_end().len();
  let whitespace_text = &preceeding_text[whitespace_start..];
  let whitespace_newline_pos = whitespace_text.rfind('\n');
  &preceeding_text[whitespace_newline_pos
    .map(|pos| whitespace_start + pos + 1)
    .unwrap_or(whitespace_start)..]
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_get_indent_text() {
    assert_eq!(get_indent_text("  hello", 2), "  ");
    assert_eq!(get_indent_text("\n  hello", 3), "  ");
    assert_eq!(get_indent_text("t\n  hello", 4), "  ");
    assert_eq!(get_indent_text("t\n\t\thello", 4), "\t\t");
    assert_eq!(get_indent_text("hello", 0), "");
    assert_eq!(get_indent_text("\nhello", 1), "");
    assert_eq!(get_indent_text("\nhello", 2), "");
  }
}
