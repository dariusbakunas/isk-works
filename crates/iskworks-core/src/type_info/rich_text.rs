//! CCP's description and bonus markup, as text and links the web app
//! renders itself. It never renders the markup as HTML.

use serde::Serialize;

/// One run of rich text.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RichSegment {
    #[serde(rename_all = "camelCase")]
    Text {
        text: String,
        bold: bool,
    },
    /// `showinfo:<typeId>`: opens that type's info.
    #[serde(rename_all = "camelCase")]
    Link {
        type_id: i64,
        text: String,
    },
    /// An `http(s)` link.
    Url {
        href: String,
        text: String,
    },
    Break,
}

/// Parses CCP markup.
///
/// - `<br>` and line breaks become [`RichSegment::Break`].
/// - `<b>` makes its text bold.
/// - `<a href=showinfo:ID>` and `<a href="http...">` become links.
/// - Any other tag (`<font>`, `<color>`, `<i>`, …) is dropped, keeping its
///   text.
/// - Entities are decoded.
///
/// Never fails: malformed markup comes out as text.
#[must_use]
pub fn parse_rich_text(markup: &str) -> Vec<RichSegment> {
    let mut out = Output::default();
    let mut rest = markup;
    while let Some(start) = rest.find('<') {
        out.text(&rest[..start]);
        let after = &rest[start + 1..];
        match after
            .find('>')
            .and_then(|end| Tag::parse(&after[..end]).map(|tag| (end, tag)))
        {
            Some((end, tag)) => {
                out.tag(tag);
                rest = &after[end + 1..];
            }
            None => {
                out.text("<");
                rest = after;
            }
        }
    }
    out.text(rest);
    out.finish()
}

/// The tags that mean something; any other well-formed tag is `Other`.
enum Tag {
    Break,
    Bold,
    BoldEnd,
    Link(Option<LinkTarget>),
    LinkEnd,
    Other,
}

#[derive(Clone)]
enum LinkTarget {
    Type(i64),
    Url(String),
}

impl Tag {
    /// `None` when `inner` (between `<` and `>`) isn't a tag, as in `a < b`.
    fn parse(inner: &str) -> Option<Self> {
        let (closing, body) = match inner.strip_prefix('/') {
            Some(body) => (true, body),
            None => (false, inner),
        };
        let name_end = body
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        let name = body[..name_end].to_ascii_lowercase();
        if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }
        Some(match (name.as_str(), closing) {
            ("br", _) => Self::Break,
            ("b", false) => Self::Bold,
            ("b", true) => Self::BoldEnd,
            ("a", false) => Self::Link(href(&body[name_end..]).and_then(link_target)),
            ("a", true) => Self::LinkEnd,
            _ => Self::Other,
        })
    }
}

/// The `href` attribute's value, quoted or not.
fn href(attributes: &str) -> Option<&str> {
    let lower = attributes.to_ascii_lowercase();
    let at = lower.find("href=")? + "href=".len();
    let value = &attributes[at..];
    match value.chars().next()? {
        quote @ ('"' | '\'') => {
            let value = &value[1..];
            Some(&value[..value.find(quote).unwrap_or(value.len())])
        }
        // Unquoted runs to whitespace: `showinfo:5//30000142` has slashes.
        _ => Some(&value[..value.find(char::is_whitespace).unwrap_or(value.len())]),
    }
}

fn link_target(href: &str) -> Option<LinkTarget> {
    if let Some(id) = href.strip_prefix("showinfo:") {
        // `showinfo:5//30000142` and friends point at non-types.
        return id
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(LinkTarget::Type);
    }
    let lower = href.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        .then(|| LinkTarget::Url(href.to_string()))
}

#[derive(Default)]
struct Output {
    segments: Vec<RichSegment>,
    bold: usize,
    /// An open `<a>`: where it goes (`None` for a target we don't link) and
    /// its text so far.
    link: Option<(Option<LinkTarget>, String)>,
}

impl Output {
    fn text(&mut self, raw: &str) {
        if raw.is_empty() {
            return;
        }
        let decoded = decode_entities(raw);
        if let Some((_, text)) = &mut self.link {
            text.push_str(&decoded.replace("\r\n", " ").replace('\n', " "));
            return;
        }
        let normalized = decoded.replace("\r\n", "\n");
        for (index, line) in normalized.split('\n').enumerate() {
            if index > 0 {
                self.segments.push(RichSegment::Break);
            }
            self.push_text(line, self.bold > 0);
        }
    }

    fn push_text(&mut self, text: &str, bold: bool) {
        if text.is_empty() {
            return;
        }
        if let Some(RichSegment::Text {
            text: previous,
            bold: previous_bold,
        }) = self.segments.last_mut()
        {
            if *previous_bold == bold {
                previous.push_str(text);
                return;
            }
        }
        self.segments.push(RichSegment::Text {
            text: text.to_string(),
            bold,
        });
    }

    fn tag(&mut self, tag: Tag) {
        match tag {
            Tag::Break => match &mut self.link {
                Some((_, text)) => text.push(' '),
                None => self.segments.push(RichSegment::Break),
            },
            Tag::Bold => self.bold += 1,
            Tag::BoldEnd => self.bold = self.bold.saturating_sub(1),
            Tag::Link(target) => {
                self.close_link();
                self.link = Some((target, String::new()));
            }
            Tag::LinkEnd => self.close_link(),
            Tag::Other => {}
        }
    }

    fn close_link(&mut self) {
        let Some((target, text)) = self.link.take() else {
            return;
        };
        match target {
            _ if text.is_empty() => {}
            Some(LinkTarget::Type(type_id)) => {
                self.segments.push(RichSegment::Link { type_id, text });
            }
            Some(LinkTarget::Url(href)) => self.segments.push(RichSegment::Url { href, text }),
            None => self.push_text(&text, self.bold > 0),
        }
    }

    fn finish(mut self) -> Vec<RichSegment> {
        self.close_link();
        self.segments
    }
}

/// Decodes the named entities CCP uses and numeric ones; anything else
/// stays as written.
fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| entity(&after[..end]).map(|c| (end, c)));
        match decoded {
            Some((end, c)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests;
