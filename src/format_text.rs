use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use crate::text_changes::TextChange;
use crate::text_changes::apply_text_changes;
use jsonc_parser::CollectOptions;
use jsonc_parser::CommentCollectionStrategy;
use jsonc_parser::ParseOptions;
use jsonc_parser::common::Ranged;
use jsonc_parser::errors::ParseError;

/// Error that occurred while formatting a Jupyter notebook.
#[derive(Debug, thiserror::Error)]
pub enum FormatTextError {
  /// The notebook could not be parsed as JSON.
  #[error(transparent)]
  Parse(#[from] ParseError),
  /// The formatter produced invalid JSON (only checked in debug builds).
  #[cfg(debug_assertions)]
  #[error(
    "dprint-plugin-jupyter produced invalid json. Please open an issue with reproduction steps at https://github.com/dprint/dprint-plugin-jupyter/issues\n{error}\n\n== TEXT ==\n{text}"
  )]
  InvalidOutput { error: ParseError, text: String },
}

/// Result returned by the host formatting callback. Any error it returns causes
/// the cell to be left unformatted, so it may be any error type.
type HostFormatResult = std::result::Result<Option<String>, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub fn format_text(
  input_text: &str,
  format_with_host: impl FnMut(&Path, String) -> HostFormatResult,
) -> Result<Option<String>, FormatTextError> {
  let had_bom = input_text.starts_with("\u{FEFF}");
  let input_text = if had_bom { &input_text[3..] } else { input_text };
  let result = format_inner(input_text, None, format_with_host)?;
  if result.is_none() && had_bom {
    Ok(Some(input_text.to_string()))
  } else {
    Ok(result)
  }
}

/// Formats only the cells the provided byte range touches.
///
/// A cell is formatted in its entirety when the range touches any part of it,
/// including its metadata and outputs, and the text outside of the sources of
/// those cells is left as it was. A range that only touches the text between
/// or outside of the cells formats nothing and one that covers the whole file
/// formats it the same as `format_text`.
///
/// Only the sources of the cells the range touches are passed to
/// `format_with_host`.
pub fn format_text_range(
  input_text: &str,
  range: Range<usize>,
  format_with_host: impl FnMut(&Path, String) -> HostFormatResult,
) -> Result<Option<String>, FormatTextError> {
  let body = input_text.strip_prefix('\u{FEFF}').unwrap_or(input_text);
  let bom_len = input_text.len() - body.len();
  let end = range.end.saturating_sub(bom_len).min(body.len());
  let range = range.start.saturating_sub(bom_len).min(end)..end;
  if range.start == 0 && range.end == body.len() {
    return format_text(input_text, format_with_host);
  }
  let result = format_inner(body, Some(&range), format_with_host)?;
  // the bom is outside of the cells, so it's kept
  Ok(result.map(|text| format!("{}{}", &input_text[..bom_len], text)))
}

fn format_inner(
  input_text: &str,
  range: Option<&Range<usize>>,
  format_with_host: impl FnMut(&Path, String) -> HostFormatResult,
) -> Result<Option<String>, FormatTextError> {
  let parse_result = jsonc_parser::parse_to_ast(
    input_text,
    &CollectOptions {
      comments: CommentCollectionStrategy::Off,
      tokens: false,
    },
    &ParseOptions {
      allow_comments: true,
      allow_loose_object_property_names: true,
      allow_trailing_commas: true,
      allow_missing_commas: true,
      allow_single_quoted_strings: true,
      allow_hexadecimal_numbers: true,
      allow_unary_plus_numbers: true,
      allow_bare_decimal_point_numbers: true,
      allow_non_finite_numbers: true,
      allow_extended_string_escapes: true,
    },
  )?;
  let Some(root_value) = parse_result.value else {
    return Ok(None);
  };

  Ok(match format_root(input_text, &root_value, range, format_with_host) {
    Some(text) => {
      #[cfg(debug_assertions)]
      validate_output_json(&text)?;
      Some(text)
    }
    None => None,
  })
}

fn format_root(
  input_text: &str,
  root_value: &jsonc_parser::ast::Value,
  range: Option<&Range<usize>>,
  mut format_with_host: impl FnMut(&Path, String) -> HostFormatResult,
) -> Option<String> {
  let root_obj = root_value.as_object()?;
  let maybe_default_language = get_metadata_language(root_obj);
  let cells = root_value.as_object()?.get_array("cells")?;

  let text_changes: Vec<TextChange> = cells
    .elements
    .iter()
    .filter(|element| range.is_none_or(|range| touches(element.start()..element.end(), range)))
    .filter_map(|element| get_cell_text_change(input_text, element, maybe_default_language, &mut format_with_host))
    .collect();

  if text_changes.is_empty() {
    None
  } else {
    Some(apply_text_changes(input_text, text_changes))
  }
}

fn touches(cell: Range<usize>, range: &Range<usize>) -> bool {
  if range.is_empty() {
    cell.start <= range.start && range.start <= cell.end
  } else {
    range.start < cell.end && range.end > cell.start
  }
}

#[cfg(debug_assertions)]
fn validate_output_json(text: &str) -> Result<(), FormatTextError> {
  // ensures the output is correct in debug mode

  let result = jsonc_parser::parse_to_ast(
    text,
    &CollectOptions {
      comments: CommentCollectionStrategy::Off,
      tokens: false,
    },
    &ParseOptions {
      allow_comments: true,
      allow_loose_object_property_names: false,
      allow_trailing_commas: true,
      allow_missing_commas: false,
      allow_single_quoted_strings: false,
      allow_hexadecimal_numbers: false,
      allow_unary_plus_numbers: false,
      allow_bare_decimal_point_numbers: false,
      // python writes NaN and Infinity into cell outputs
      allow_non_finite_numbers: true,
      allow_extended_string_escapes: false,
    },
  );
  match result {
    Ok(_) => Ok(()),
    Err(error) => Err(FormatTextError::InvalidOutput {
      error,
      text: text.to_string(),
    }),
  }
}

fn get_cell_text_change(
  file_text: &str,
  cell: &jsonc_parser::ast::Value,
  maybe_default_language: Option<&str>,
  format_with_host: &mut impl FnMut(&Path, String) -> HostFormatResult,
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

  let new_text = if code_block.is_array {
    build_array_json_text(formatted_text, code_block.indent_text)
  } else {
    serde_json::to_string(&formatted_text).unwrap()
  };

  Some(TextChange {
    range: code_block.replace_range,
    new_text,
  })
}

struct CodeBlockText<'a> {
  // Can be either a string or an array of strings.
  // (https://github.com/jupyter/nbformat/blob/0708dd627d9ef81b12f231defb0d94dd7e80e3f4/nbformat/v4/nbformat.v4.5.schema.json#L460C7-L468C8)
  is_array: bool,
  indent_text: &'a str,
  replace_range: Range<usize>,
  source: String,
}

fn analyze_code_block<'a>(cell: &jsonc_parser::ast::Object<'a>, file_text: &'a str) -> Option<CodeBlockText<'a>> {
  let mut indent_text = "";
  let mut replace_range = Range::default();
  let mut is_array = false;
  let cell_source = match &cell.get("source")?.value {
    jsonc_parser::ast::Value::Array(items) => {
      is_array = true;
      if items.elements.is_empty() {
        // there's no string to replace
        return None;
      }
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
    jsonc_parser::ast::Value::StringLit(string) => {
      replace_range = string.range.start..string.range.end;
      string.value.to_string()
    }
    _ => return None,
  };
  Some(CodeBlockText {
    is_array,
    indent_text,
    replace_range,
    source: cell_source,
  })
}

/// Turn the formatted text into a json array, split up by line breaks.
fn build_array_json_text(formatted_text: &str, indent_text: &str) -> String {
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

/// Gets the virtual file path used to format a cell's code with the host.
///
/// The dprint CLI selects the plugin based on this path, so a cell is formatted
/// whenever a plugin handles the path's extension. When no plugin does, the host
/// leaves the text as-is and the cell stays unformatted.
fn language_to_path(language: &str) -> Option<PathBuf> {
  let language = language.to_ascii_lowercase();
  let ext = match known_language_extension(&language) {
    Some(ext) => ext,
    // fall back to the language id itself as the extension (ex. sql, toml, go)
    None if is_fallback_extension(&language) => &language,
    None => return None,
  };
  Some(PathBuf::from(format!("code_block.{}", ext)))
}

/// Gets the file extension for languages (VS Code language ids and Jupyter
/// kernel language names) whose conventional extension differs from the id.
fn known_language_extension(language: &str) -> Option<&'static str> {
  Some(match language {
    "bash" | "sh" | "shell" | "shellscript" => "sh",
    "c#" | "csharp" => "cs",
    "c++" | "cpp" => "cpp",
    "clojure" => "clj",
    "coffeescript" => "coffee",
    "elixir" => "ex",
    "erlang" => "erl",
    "f#" | "fsharp" => "fs",
    "handlebars" => "hbs",
    "haskell" => "hs",
    "javascript" => "js",
    "javascriptreact" => "jsx",
    "julia" => "jl",
    "kotlin" => "kt",
    "latex" => "tex",
    "markdown" => "md",
    "nushell" => "nu",
    "ocaml" => "ml",
    "perl" => "pl",
    "powershell" => "ps1",
    "proto3" | "protobuf" => "proto",
    "python" | "python3" => "py",
    "restructuredtext" => "rst",
    "ruby" => "rb",
    "rust" => "rs",
    "terraform" => "tf",
    "typescript" => "ts",
    "typescriptreact" => "tsx",
    "yaml" => "yml",
    _ => return None,
  })
}

fn is_fallback_extension(language: &str) -> bool {
  !language.is_empty()
    && language.chars().all(|c| c.is_ascii_alphanumeric())
    // never format a cell as a notebook, which would recurse into this plugin
    && language != "ipynb"
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

  #[test]
  fn test_language_to_path() {
    fn assert_path(language: &str, expected: Option<&str>) {
      assert_eq!(
        language_to_path(language),
        expected.map(PathBuf::from),
        "language: {}",
        language
      );
    }

    // known languages whose extension differs from the language id
    assert_path("python", Some("code_block.py"));
    assert_path("Python3", Some("code_block.py"));
    assert_path("typescript", Some("code_block.ts"));
    assert_path("typescriptreact", Some("code_block.tsx"));
    assert_path("javascript", Some("code_block.js"));
    assert_path("javascriptreact", Some("code_block.jsx"));
    assert_path("markdown", Some("code_block.md"));
    assert_path("rust", Some("code_block.rs"));
    assert_path("c++", Some("code_block.cpp"));
    assert_path("c#", Some("code_block.cs"));
    assert_path("csharp", Some("code_block.cs"));
    assert_path("f#", Some("code_block.fs"));
    assert_path("shellscript", Some("code_block.sh"));
    assert_path("bash", Some("code_block.sh"));
    assert_path("powershell", Some("code_block.ps1"));
    assert_path("yaml", Some("code_block.yml"));
    assert_path("perl", Some("code_block.pl"));
    assert_path("terraform", Some("code_block.tf"));

    // other languages use the language id as the extension
    assert_path("sql", Some("code_block.sql"));
    assert_path("SQL", Some("code_block.sql"));
    assert_path("toml", Some("code_block.toml"));
    assert_path("css", Some("code_block.css"));
    assert_path("json", Some("code_block.json"));
    assert_path("jsonc", Some("code_block.jsonc"));
    assert_path("go", Some("code_block.go"));
    assert_path("dockerfile", Some("code_block.dockerfile"));

    // languages that can't be used as an extension
    assert_path("", None);
    assert_path("objective-c", None);
    assert_path("some.lang", None);
    assert_path("../lang", None);
    assert_path("lang ", None);
    assert_path("ipynb", None);
  }

  #[test]
  fn formats_with_bom() {
    // no changes to code other than bom
    {
      let input_text = "\u{FEFF}{\"cells\":[{\"cell_type\":\"code\",\"source\":\"let x = 5;\"}]}";
      let formatted_text = format_text(input_text, |_, text| Ok(Some(text))).unwrap().unwrap();
      assert_eq!(
        formatted_text,
        "{\"cells\":[{\"cell_type\":\"code\",\"source\":\"let x = 5;\"}]}"
      );
    }
    // other changes as well
    let input_text = "\u{FEFF}{
  \"cells\":[{
    \"cell_type\":\"code\",
    \"metadata\": {
      \"vscode\": {
       \"languageId\": \"typescript\"
      }
    },
    \"source\": \"let x = 5;\"
  }]
}
";
    let formatted_text = format_text(input_text, |_, text| Ok(Some(format!("{}_formatted", text))))
      .unwrap()
      .unwrap();
    assert_eq!(
      formatted_text,
      "{
  \"cells\":[{
    \"cell_type\":\"code\",
    \"metadata\": {
      \"vscode\": {
       \"languageId\": \"typescript\"
      }
    },
    \"source\": \"let x = 5;_formatted\"
  }]
}
"
    );
  }

  #[test]
  fn formats_range_with_bom() {
    // the spec files can't express this since editors strip the bom
    let input_text = "\u{FEFF}{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b\"}]}";
    let format = |range: Range<usize>| {
      let mut calls = Vec::new();
      let output = format_text_range(input_text, range, |_, text| {
        calls.push(text.clone());
        Ok(Some(format!("{}_formatted", text)))
      })
      .unwrap();
      (calls, output)
    };

    // these ranges are in the first cell when not accounting for the bom
    let start = input_text.find("\"b\"").unwrap();
    let second_cell_start = input_text.find("},{").unwrap() + 2;
    for range in [start..start + 1, second_cell_start..second_cell_start + 3] {
      let (calls, output) = format(range);
      assert_eq!(calls, vec!["b"]);
      assert_eq!(
        output.as_deref(),
        Some(
          "\u{FEFF}{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b_formatted\"}]}"
        )
      );
    }

    // the last byte of the first cell, which is the comma when not accounting for the bom
    let (calls, output) = format(second_cell_start - 2..second_cell_start - 1);
    assert_eq!(calls, vec!["a"]);
    assert_eq!(
      output.as_deref(),
      Some(
        "\u{FEFF}{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a_formatted\"},{\"cell_type\":\"markdown\",\"source\":\"b\"}]}"
      )
    );

    // within the first cell and the bom
    let (calls, output) = format(1..20);
    assert_eq!(calls, vec!["a"]);
    assert_eq!(
      output.as_deref(),
      Some(
        "\u{FEFF}{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a_formatted\"},{\"cell_type\":\"markdown\",\"source\":\"b\"}]}"
      )
    );

    let (calls, output) = format(start..start + 1);
    assert_eq!(calls, vec!["b"]);
    assert_eq!(
      output.as_deref(),
      Some(
        "\u{FEFF}{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b_formatted\"}]}"
      )
    );

    // nothing touched, so the bom is left alone
    for range in [0..0, 0..3, 1..2, 0..4] {
      let (calls, output) = format(range);
      assert!(calls.is_empty());
      assert_eq!(output, None);
    }

    // the whole file is formatted the same as when not providing a range
    for range in [0..input_text.len(), 3..input_text.len(), 0..usize::MAX] {
      let (calls, output) = format(range);
      assert_eq!(calls, vec!["a", "b"]);
      assert_eq!(
        output,
        format_text(input_text, |_, text| Ok(Some(format!("{}_formatted", text)))).unwrap()
      );
      assert!(!output.unwrap().starts_with('\u{FEFF}'));
    }
    let unchanged = |range: Range<usize>| format_text_range(input_text, range, |_, _| Ok(None)).unwrap();
    assert_eq!(unchanged(0..input_text.len()).as_deref(), Some(&input_text[3..]));
    assert_eq!(unchanged(start..start + 1), None);
  }

  #[test]
  fn formats_range_out_of_bounds() {
    let input_text =
      "{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b\"}]}";
    let len = input_text.len();
    let second_cell_start = input_text.find("},{").unwrap() + 2;
    let format = |range: Range<usize>| {
      let mut calls = Vec::new();
      format_text_range(input_text, range, |_, text| {
        calls.push(text);
        Ok(None)
      })
      .unwrap();
      calls
    };

    // past the end
    assert!(format(len..len).is_empty());
    assert!(format(len + 5..len + 10).is_empty());
    assert!(format(usize::MAX..usize::MAX).is_empty());
    // from within the file to past the end
    assert_eq!(format(second_cell_start..len + 10), vec!["b"]);
    assert_eq!(format(second_cell_start..usize::MAX), vec!["b"]);
    assert_eq!(format(1..usize::MAX), vec!["a", "b"]);
    // a start after the end is a cursor at the end
    #[allow(clippy::reversed_empty_ranges)]
    {
      assert_eq!(format(len..second_cell_start), vec!["b"]);
      assert_eq!(format(usize::MAX..second_cell_start - 1), vec!["a"]);
      assert!(format(len..2).is_empty());
    }
  }

  #[test]
  fn formats_range_with_carriage_return_line_feeds() {
    // the spec files can't express this since they normalize line endings
    let input_text = "{\r\n \"cells\": [\r\n  {\r\n   \"cell_type\": \"markdown\",\r\n   \"source\": [\r\n    \"a\\r\\n\",\r\n    \"b\"\r\n   ]\r\n  },\r\n  {\r\n   \"cell_type\": \"markdown\",\r\n   \"source\": \"c\"\r\n  }\r\n ]\r\n}\r\n";
    let start = input_text.find("\"b\"").unwrap();
    let mut calls = Vec::new();
    let output = format_text_range(input_text, start..start + 1, |_, text| {
      calls.push(text.clone());
      Ok(Some(format!("{}_formatted\r\n", text)))
    })
    .unwrap();
    assert_eq!(calls, vec!["a\r\nb"]);
    assert_eq!(
      output.as_deref(),
      Some(
        "{\r\n \"cells\": [\r\n  {\r\n   \"cell_type\": \"markdown\",\r\n   \"source\": [\r\n    \"a\\r\\n\",\n    \"b_formatted\"\r\n   ]\r\n  },\r\n  {\r\n   \"cell_type\": \"markdown\",\r\n   \"source\": \"c\"\r\n  }\r\n ]\r\n}\r\n"
      )
    );
  }

  #[test]
  fn formats_range_with_tab_indentation() {
    // tabs are hard to see in the spec files
    let input_text = "{\n\t\"cells\": [\n\t\t{\n\t\t\t\"cell_type\": \"markdown\",\n\t\t\t\"source\": [\n\t\t\t\t\"a\"\n\t\t\t]\n\t\t},\n\t\t{\n\t\t\t\"cell_type\": \"markdown\",\n\t\t\t\"source\": [\n\t\t\t\t\"b\"\n\t\t\t]\n\t\t}\n\t]\n}\n";
    let start = input_text.find("\"b\"").unwrap();
    let output = format_text_range(input_text, start..start + 1, |_, text| {
      Ok(Some(format!("{}\n\tformatted\n", text)))
    })
    .unwrap();
    assert_eq!(
      output.as_deref(),
      Some(
        "{\n\t\"cells\": [\n\t\t{\n\t\t\t\"cell_type\": \"markdown\",\n\t\t\t\"source\": [\n\t\t\t\t\"a\"\n\t\t\t]\n\t\t},\n\t\t{\n\t\t\t\"cell_type\": \"markdown\",\n\t\t\t\"source\": [\n\t\t\t\t\"b\\n\",\n\t\t\t\t\"\\tformatted\"\n\t\t\t]\n\t\t}\n\t]\n}\n"
      )
    );
  }

  #[test]
  fn formats_range_only_passing_touched_cells_to_host() {
    let input_text = "{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"code\",\"metadata\":{\"vscode\":{\"languageId\":\"typescript\"}},\"source\":[\"b\\n\",\"c\"]},{\"cell_type\":\"markdown\",\"source\":\"d\"}]}";
    let start = input_text.find("\"c\"").unwrap();
    let mut calls = Vec::new();
    let output = format_text_range(input_text, start..start + 1, |path, text| {
      calls.push((path.to_path_buf(), text));
      Ok(None)
    })
    .unwrap();
    assert_eq!(calls, vec![(PathBuf::from("code_block.ts"), "b\nc".to_string())]);
    assert_eq!(output, None);
  }

  #[test]
  fn formats_range_when_host_errors() {
    let input_text = "{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b\"},{\"cell_type\":\"markdown\",\"source\":\"c\"}]}";
    let start = input_text.find("\"a\"").unwrap();
    let end = input_text.find("\"b\"").unwrap() + 1;
    let output = format_text_range(input_text, start..end, |_, text| {
      if text == "a" {
        Err("failed".into())
      } else {
        Ok(Some(format!("{}_formatted", text)))
      }
    })
    .unwrap();
    assert_eq!(
      output.as_deref(),
      Some(
        "{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"},{\"cell_type\":\"markdown\",\"source\":\"b_formatted\"},{\"cell_type\":\"markdown\",\"source\":\"c\"}]}"
      )
    );
  }

  #[test]
  fn formats_range_of_empty_or_invalid_text() {
    for text in ["", " ", "// comment"] {
      for range in [0..0, 0..1, 5..10] {
        let output = format_text_range(text, range, |_, _| panic!("should not format")).unwrap();
        assert_eq!(output, None, "text: {:?}", text);
      }
    }

    let text = "{\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"a\"}";
    let start = text.find("\"a\"").unwrap();
    let result = format_text_range(text, start..start + 1, |_, _| panic!("should not format"));
    assert!(matches!(result, Err(FormatTextError::Parse(_))));
  }

  #[test]
  fn formats_with_non_finite_numbers() {
    let input_text = "{\"cells\":[{\"cell_type\":\"code\",\"metadata\":{\"vscode\":{\"languageId\":\"typescript\"}},\"outputs\":[NaN,Infinity,-Infinity],\"source\":\"let x = 5;\"}]}";
    let formatted_text = format_text(input_text, |_, text| Ok(Some(format!("{}_formatted", text))))
      .unwrap()
      .unwrap();
    assert_eq!(
      formatted_text,
      "{\"cells\":[{\"cell_type\":\"code\",\"metadata\":{\"vscode\":{\"languageId\":\"typescript\"}},\"outputs\":[NaN,Infinity,-Infinity],\"source\":\"let x = 5;_formatted\"}]}"
    );
  }
}
