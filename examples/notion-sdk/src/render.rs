//! Plain-text outline rendering. This is not HTML or a complete Notion/Markdown renderer.
//! Styling is omitted, links are not followed, and unknown types emit diagnostic markers.

use crate::models::{Block, BlockValue, Property, PropertyValue, RichText};

/// Concatenates text and equation expressions; unknown fragments emit an explicit marker.
pub fn rich_text(fragments: &[RichText]) -> String {
    fragments
        .iter()
        .map(|fragment| match fragment {
            RichText::Text(text) => text.content.clone(),
            RichText::Equation(equation) => format!("${}$", equation.expression),
            RichText::Unknown { kind, .. } => format!("[unsupported rich text: {kind}]"),
        })
        .collect()
}

/// Renders one property as plain text. Null supported values render as an empty string.
/// Returned text is a partial page-property representation, never a completeness guarantee.
pub fn property(property: &Property) -> String {
    match &property.value {
        PropertyValue::Title(text) | PropertyValue::RichText(text) => rich_text(text),
        PropertyValue::Number(number) => number.map(|n| n.to_string()).unwrap_or_default(),
        PropertyValue::Checkbox(checked) => checked.to_string(),
        PropertyValue::Select(option) => {
            option.as_ref().map(|o| o.name.clone()).unwrap_or_default()
        }
        PropertyValue::Date(date) => date
            .as_ref()
            .map(|date| match &date.end {
                Some(end) => format!("{}..{end}", date.start),
                None => date.start.clone(),
            })
            .unwrap_or_default(),
        PropertyValue::Unknown { kind, .. } => format!("[unsupported property: {kind}]"),
    }
}

/// Renders this block only. `has_children` is not consumed: callers must fetch descendants
/// separately, including descendants of unknown block types.
pub fn block(block: &Block) -> String {
    match &block.value {
        BlockValue::Paragraph(text) => rich_text(&text.rich_text),
        BlockValue::Heading1(text) => format!("# {}", rich_text(&text.rich_text)),
        BlockValue::Heading2(text) => format!("## {}", rich_text(&text.rich_text)),
        BlockValue::Heading3(text) => format!("### {}", rich_text(&text.rich_text)),
        BlockValue::BulletedListItem(text) => format!("- {}", rich_text(&text.rich_text)),
        BlockValue::ToDo(todo) => format!(
            "- [{}] {}",
            if todo.checked { "x" } else { " " },
            rich_text(&todo.rich_text)
        ),
        BlockValue::Divider => "---".to_owned(),
        BlockValue::Unknown { kind, .. } => format!("[unsupported block: {kind}]"),
    }
}
