//! @acp:module "Primer Renderer"
//! @acp:summary "Template rendering for primer output"
//! @acp:domain cli
//! @acp:layer output

use anyhow::{anyhow, Result};

use super::condition::ProjectState;
use super::dynamic::get_dynamic_data;
use super::types::*;

/// Output format for primer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Markdown,
    Compact,
    Json,
    Text,
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "markdown" | "md" => Ok(OutputFormat::Markdown),
            "compact" => Ok(OutputFormat::Compact),
            "json" => Ok(OutputFormat::Json),
            "text" | "txt" => Ok(OutputFormat::Text),
            _ => Err(anyhow!("Unknown output format: {}", s)),
        }
    }
}

/// Render primer output from selected sections
pub fn render_primer(
    sections: &[SelectedSection],
    format: OutputFormat,
    project_state: &ProjectState,
) -> Result<String> {
    if format == OutputFormat::Json {
        return render_json(sections);
    }

    let mut output = String::new();

    for section in sections {
        let rendered = render_section(&section.section, format, project_state)?;
        if !rendered.is_empty() {
            output.push_str(&rendered);
            output.push_str("\n\n");
        }
    }

    Ok(output.trim().to_string())
}

/// Render a single section
fn render_section(
    section: &Section,
    format: OutputFormat,
    state: &ProjectState,
) -> Result<String> {
    // Get template with fallback chain
    let template = match section.formats.get(format) {
        Some(t) => t,
        None => return Ok(String::new()), // No template available
    };

    if let Some(ref data_config) = section.data {
        // Dynamic section with list data
        let items = get_dynamic_data(data_config, state);
        if items.is_empty() {
            // Check empty behavior
            let empty_behavior = data_config.empty_behavior.as_deref().unwrap_or("exclude");
            match empty_behavior {
                "exclude" => return Ok(String::new()),
                "placeholder" => {
                    return Ok(template.empty_template.clone().unwrap_or_default())
                }
                _ => return Ok(String::new()),
            }
        }
        render_list_template(template, &items)
    } else {
        // Static section
        render_static_template(template)
    }
}

fn render_static_template(template: &FormatTemplate) -> Result<String> {
    Ok(template.template.clone().unwrap_or_default())
}

fn render_list_template(template: &FormatTemplate, items: &[DynamicItem]) -> Result<String> {
    let mut output = String::new();

    // Add header if present
    if let Some(ref header) = template.header {
        output.push_str(header);
    }

    // Render each item
    let separator = template.separator.as_deref().unwrap_or("\n");
    let item_template = template.item_template.as_deref().unwrap_or("{{item}}");

    let rendered_items: Vec<String> = items
        .iter()
        .map(|item| render_item(item_template, item))
        .collect();

    output.push_str(&rendered_items.join(separator));

    // Add footer if present
    if let Some(ref footer) = template.footer {
        output.push_str(footer);
    }

    Ok(output)
}

fn render_item(template: &str, item: &DynamicItem) -> String {
    let mut result = template.to_string();

    match item {
        DynamicItem::ProtectedFile(file) => {
            result = result.replace("{{path}}", &file.path);
            result = result.replace("{{level}}", &file.level);
            result = result.replace("{{#if reason}}", "");
            result = result.replace("{{/if}}", "");
            result = result.replace("{{reason}}", file.reason.as_deref().unwrap_or(""));
        }
        DynamicItem::Domain(domain) => {
            result = result.replace("{{name}}", &domain.name);
            result = result.replace("{{pattern}}", &domain.pattern);
            result = result.replace(
                "{{description}}",
                domain.description.as_deref().unwrap_or(""),
            );
        }
        DynamicItem::Layer(layer) => {
            result = result.replace("{{name}}", &layer.name);
            result = result.replace("{{pattern}}", &layer.pattern);
        }
        DynamicItem::EntryPoint(entry) => {
            result = result.replace("{{name}}", &entry.name);
            result = result.replace("{{path}}", &entry.path);
        }
        DynamicItem::Variable(var) => {
            result = result.replace("{{name}}", &var.name);
            result = result.replace("{{value}}", &var.value);
            result = result.replace(
                "{{description}}",
                var.description.as_deref().unwrap_or(""),
            );
        }
        DynamicItem::Attempt(attempt) => {
            result = result.replace("{{id}}", &attempt.id);
            result = result.replace("{{problem}}", &attempt.problem);
            result = result.replace("{{attemptCount}}", &attempt.attempt_count.to_string());
        }
        DynamicItem::Hack(hack) => {
            result = result.replace("{{file}}", &hack.file);
            result = result.replace("{{reason}}", &hack.reason);
            result = result.replace("{{expires}}", hack.expires.as_deref().unwrap_or(""));
        }
    }

    // Simple conditional removal for {{#if ...}}...{{/if}} blocks with empty values
    // This is a simplified version - a full Handlebars implementation would be more robust
    result = remove_empty_conditionals(&result);

    result
}

fn remove_empty_conditionals(s: &str) -> String {
    // Very simple: if the result still has {{#if and {{/if}}, just strip them
    let mut result = s.to_string();
    
    // Remove empty {{#if reason}}...{{/if}} blocks
    while let Some(start) = result.find("{{#if ") {
        if let Some(end) = result[start..].find("{{/if}}") {
            let block = &result[start..start + end + 7];
            // Check if the conditional content is empty
            let content_start = block.find("}}").map(|i| i + 2).unwrap_or(0);
            let content_end = block.rfind("{{/if}}").unwrap_or(block.len());
            let content = &block[content_start..content_end];
            if content.trim().is_empty() || content.contains(": {{") {
                result = result.replace(block, "");
            } else {
                // Keep the content, remove the conditional markers
                let clean_content = content.to_string();
                result = result.replace(block, &clean_content);
            }
        } else {
            break;
        }
    }
    
    result
}

fn render_json(sections: &[SelectedSection]) -> Result<String> {
    #[derive(serde::Serialize)]
    struct JsonOutput {
        total_tokens: u32,
        sections_included: usize,
        sections: Vec<JsonSection>,
    }

    #[derive(serde::Serialize)]
    struct JsonSection {
        id: String,
        category: String,
        tokens: u32,
        value: f64,
    }

    let output = JsonOutput {
        total_tokens: sections.iter().map(|s| s.tokens).sum(),
        sections_included: sections.len(),
        sections: sections
            .iter()
            .map(|s| JsonSection {
                id: s.id.clone(),
                category: s.section.category.clone(),
                tokens: s.tokens,
                value: s.value,
            })
            .collect(),
    };

    serde_json::to_string_pretty(&output).map_err(Into::into)
}

/// Dynamic item types for rendering
#[derive(Debug, Clone)]
pub enum DynamicItem {
    ProtectedFile(super::condition::ProtectedFile),
    Domain(super::condition::DomainInfo),
    Layer(super::condition::LayerInfo),
    EntryPoint(super::condition::EntryPointInfo),
    Variable(super::condition::VariableInfo),
    Attempt(super::condition::AttemptInfo),
    Hack(super::condition::HackInfo),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parse() {
        assert_eq!("markdown".parse::<OutputFormat>().unwrap(), OutputFormat::Markdown);
        assert_eq!("compact".parse::<OutputFormat>().unwrap(), OutputFormat::Compact);
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
    }

    #[test]
    fn test_render_static_template() {
        let template = FormatTemplate {
            template: Some("Hello, world!".to_string()),
            ..Default::default()
        };
        assert_eq!(render_static_template(&template).unwrap(), "Hello, world!");
    }
}
