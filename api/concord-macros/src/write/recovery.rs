//! Attribute each primary refusal to its smallest enclosing declaration.
use super::*;

// On stable Rust, joining spans from several native tokens can fail. Inspect
// their locations separately instead of relying on a synthetic joined span.
fn contains(tokens: TokenStream, position: proc_macro2::LineColumn) -> bool {
    tokens.into_iter().any(|token| {
        if let proc_macro2::TokenTree::Group(group) = &token {
            if contains(group.stream(), position) {
                return true;
            }
        }
        token.span().start() <= position && position < token.span().end()
    })
}

fn last_location(tokens: TokenStream) -> Option<proc_macro2::LineColumn> {
    tokens
        .into_iter()
        .map(|token| {
            let end = token.span().end();
            if let proc_macro2::TokenTree::Group(group) = token {
                last_location(group.stream()).map_or(end, |inner| inner.max(end))
            } else {
                end
            }
        })
        .max()
}

fn range(node: &Node) -> (proc_macro2::LineColumn, proc_macro2::LineColumn) {
    let start = node.span.start();
    let end = if let Some(braces) = &node.braces {
        braces.span.close().end()
    } else if let Some(output) = &node.output {
        last_location(output.ty.to_token_stream()).unwrap_or_else(|| node.name.span().end())
    } else if let Some(signature) = &node.signature {
        signature.parens.span.close().end()
    } else {
        node.name.span().end()
    };
    (start, end)
}

fn owner(node: &Node, span: Span) -> &Node {
    let position = span.start();
    for child in node.children.iter().chain(&node.parts) {
        let (start, end) = range(child);
        if start <= position && position <= end {
            return owner(child, span);
        }
    }
    node
}

/// Retain declarations independent of refused siblings; facts under damage are
/// deliberately not guessed. The original diagnostic is emitted separately.
pub(crate) fn sound_client<'a>(
    client: &'a Client,
    model: &'a flat::Client<'a>,
    diagnostics: &crate::diag::Diagnostics,
) -> TokenStream {
    if diagnostics.is_empty() {
        return super::client(client, model);
    }
    let mut refused = HashSet::new();
    for &span in diagnostics.primary_spans() {
        let node = owner(&client.node, span);
        if std::ptr::eq(node, &client.node) {
            // The client core, aliases and settings all depend on this type.
            // A refused problem declaration leaves no sound client shell.
            if client.node.statements.iter().any(|statement| {
                matches!(&statement.kind, StatementKind::Problem { ty, .. }
                    if contains(ty.to_token_stream(), span.start()))
            }) {
                return TokenStream::new();
            }
            // A refused root header cannot supply any sound public client.
            let header_end = client.node.signature.as_ref().map_or_else(
                || client.node.name.span().end(),
                |s| s.parens.span.close().end(),
            );
            if span.start() <= header_end {
                return TokenStream::new();
            }
            // A root statement is inherited everywhere. Keep the client shell.
            refused.extend(client.node.children.iter().map(|n| n as *const Node));
        } else {
            refused.insert(node as *const Node);
            if matches!(node.kind, NodeKind::Part) {
                // Every endpoint that names a damaged part depends on that part.
                for level in &model.levels {
                    if level
                        .statements
                        .iter()
                        .any(|s| std::ptr::eq(s.origin.written, node))
                        || level.inputs.iter().any(|i| std::ptr::eq(i.written, node))
                    {
                        refused.insert(level.node as *const Node);
                    }
                }
            }
        }
    }
    super::recovered(client, model, refused)
}
