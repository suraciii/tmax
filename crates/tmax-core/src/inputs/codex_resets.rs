use crate::{
    Error, Result,
    config::{Config, InputDeclaration, Provider},
};

use super::exact::ExactInstant;

pub(super) fn capability(provider: &Provider, declaration: &InputDeclaration) -> bool {
    declaration.source == "codex-resets"
        && declaration.kind == "reset"
        && declaration.scope == "codex"
        && crate::provider_integration::integration_id(provider).ok() == Some("codex-subscription")
}

#[derive(Debug)]
pub(super) struct Forecast {
    pub(super) reset_at: ExactInstant,
    pub(super) reference: String,
}
fn parse_markdown(text: &str) -> Result<Forecast> {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
    let fail = || {
        Error::new(
            "source_format_error",
            "Unsupported Codex Resets Markdown representation",
        )
    };
    let mut title = 0;
    let mut sections = 0;
    let mut in_section = false;
    let mut heading = None;
    let mut heading_text = String::new();
    let mut quote_depth = 0usize;
    let mut list_depth = 0usize;
    let mut timestamps = 0usize;
    let mut paragraph = false;
    let mut paragraph_text = String::new();
    let mut codes = Vec::new();
    let mut paragraphs = Vec::new();
    let mut links = Vec::new();
    let mut current_link: Option<(String, String)> = None;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => list_depth = list_depth.saturating_sub(1),
            Event::Start(Tag::Heading { level, .. }) if quote_depth == 0 && list_depth == 0 => {
                heading = Some(level);
                heading_text.clear();
                if level == HeadingLevel::H2 {
                    in_section = false;
                }
            }
            Event::End(TagEnd::Heading(level)) if heading == Some(level) => {
                if level == HeadingLevel::H1 && heading_text == "Codex Resets" {
                    title += 1;
                }
                if level == HeadingLevel::H2 && heading_text == "Reset scheduled" {
                    sections += 1;
                    in_section = true;
                }
                heading = None;
            }
            Event::Start(Tag::Paragraph) if in_section && quote_depth == 0 && list_depth == 0 => {
                paragraph = true;
                paragraph_text.clear();
                codes.clear();
            }
            Event::End(TagEnd::Paragraph) if paragraph => {
                if paragraph_text
                    .split('·')
                    .any(|p| p.trim() == "Upcoming reset")
                {
                    paragraphs.push((paragraph_text.clone(), codes.clone()));
                }
                paragraph = false;
            }
            Event::Start(Tag::Link { dest_url, .. })
                if in_section && quote_depth == 0 && list_depth == 0 =>
            {
                if current_link.is_some() {
                    return Err(fail());
                }
                current_link = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, label)) = current_link.take()
                    && label == "View announcement"
                {
                    links.push(url);
                }
            }
            Event::Text(text) => {
                if heading.is_some() {
                    heading_text.push_str(&text);
                }
                if paragraph {
                    paragraph_text.push_str(&text);
                }
                if let Some((_, label)) = &mut current_link {
                    label.push_str(&text);
                }
            }
            Event::Code(text) => {
                if in_section
                    && quote_depth == 0
                    && list_depth == 0
                    && text.ends_with('Z')
                    && ExactInstant::parse(&text).is_ok()
                {
                    timestamps += 1;
                }
                if paragraph {
                    paragraph_text.push_str(&text);
                    codes.push(text.to_string());
                }
            }
            Event::SoftBreak | Event::HardBreak if paragraph => {
                paragraph_text.push(' ');
            }
            _ => {}
        }
    }
    if title != 1 || sections != 1 || paragraphs.len() != 1 || links.len() != 1 || timestamps != 1 {
        return Err(fail());
    }
    let (paragraph, codes) = paragraphs.pop().unwrap();
    if codes.len() != 1
        || !codes[0].ends_with('Z')
        || paragraph
            .split('·')
            .filter(|part| part.trim() == codes[0])
            .count()
            != 1
    {
        return Err(fail());
    }
    let reset_at = ExactInstant::parse(&codes[0]).map_err(|_| fail())?;
    let reference = links.pop().unwrap();
    if !reference
        .strip_prefix("https://x.com/thsottiaux/status/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(fail());
    }
    Ok(Forecast {
        reset_at,
        reference,
    })
}
pub(super) async fn fetch_forecast(config: &Config) -> Result<Forecast> {
    use futures_util::StreamExt;
    let client = crate::provider_integration::client(config).map_err(|_| {
        Error::new(
            "transport_error",
            "Cannot initialize the configured input transport",
        )
    })?;
    let response = client
        .get("https://codex-resets.com/index.md")
        .header(reqwest::header::ACCEPT, "text/markdown")
        .send()
        .await
        .map_err(|_| Error::new("transport_error", "Codex announcement transport failed"))?;
    if !response.status().is_success()
        || response.url().as_str() != "https://codex-resets.com/index.md"
    {
        return Err(Error::new(
            "transport_error",
            "Codex announcement response or redirect was refused",
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if content_type != "text/markdown" {
        return Err(Error::new(
            "source_format_error",
            "Codex announcement is not qualified Markdown",
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            Error::new("transport_error", "Codex announcement body was interrupted")
        })?;
        if bytes.len().saturating_add(chunk.len()) > 1024 * 1024 {
            return Err(Error::new(
                "source_format_error",
                "Codex announcement exceeds the bounded source size",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("source_format_error", "Codex announcement is not UTF-8"))?;
    parse_markdown(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qualified_markdown_rejects_ambiguity_and_quoted_forecasts() {
        let page = "# Codex Resets\n\n## Reset scheduled\n\n**in 9 hours** · `2026-10-23T17:00:00.000Z` · Upcoming reset · Regular reset\n\n> `2026-10-24T00:00:00Z` · Upcoming reset\n\n[View announcement](https://x.com/thsottiaux/status/123)\n\n## History\n\n`2020-01-01T00:00:00Z`\n";
        assert_eq!(
            parse_markdown(page).unwrap().reset_at.canonical(),
            "2026-10-23T17:00:00Z"
        );
        assert!(parse_markdown(&page.replace("## History", "## Reset scheduled")).is_err());
        assert!(
            parse_markdown(&page.replace(" · Regular reset", " · `2026-10-24T00:00:00Z`")).is_err()
        );
        assert!(parse_markdown(&page.replace("status/123", "status/123?token=x")).is_err());
        assert!(parse_markdown(&page.replace("https://x.com/", "https://x.com:443/")).is_err());
    }
}
