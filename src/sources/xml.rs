//! A small XML reader, enough for Atom and OPDS catalog feeds: elements, attributes, text,
//! character references and CDATA. Namespace prefixes are kept as written ("dc:language");
//! `Element::local` gives the name without one.

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    /// The name without its namespace prefix.
    pub fn local(&self) -> &str {
        local(&self.name)
    }

    /// An attribute by its local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| local(k) == name).map(|(_, v)| v.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// Child elements with this local name.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.local() == name)
    }

    pub fn first(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.local() == name)
    }

    /// All the text inside, trimmed, with runs of white space collapsed.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn collect_text(&self, out: &mut String) {
        for n in &self.children {
            match n {
                Node::Text(t) => out.push_str(t),
                // Inline markup sits inside words and sentences; <wbr> only marks where a long
                // word may break.
                Node::Element(e) if INLINE.contains(&e.local()) => e.collect_text(out),
                Node::Element(e) => {
                    out.push(' ');
                    e.collect_text(out);
                    out.push(' ');
                }
            }
        }
    }

    /// All the text inside exactly as written (HTML carried in CDATA, say).
    pub fn raw_text(&self) -> String {
        let mut out = String::new();
        for n in &self.children {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => out.push_str(&e.raw_text()),
            }
        }
        out
    }

    /// Text of the first child with this local name.
    pub fn child_text(&self, name: &str) -> Option<String> {
        self.first(name).map(Element::text).filter(|t| !t.is_empty())
    }
}

/// Elements that run on within text (XHTML in summaries).
pub(crate) const INLINE: [&str; 14] = ["a", "abbr", "b", "cite", "code", "em", "i", "q", "s", "small", "span", "strong", "sub", "wbr"];

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Replaces character and entity references.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{A0}'),
            _ if name.starts_with("#x") || name.starts_with("#X") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
            _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parses a document and returns its root element.
pub fn parse(xml: &str) -> Result<Element, String> {
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut rest = xml.trim_start_matches('\u{FEFF}');
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut stack, rest);
            break;
        };
        if lt > 0 {
            push_text(&mut stack, &rest[..lt]);
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").ok_or("unterminated CDATA")?;
            if let Some(top) = stack.last_mut() {
                top.children.push(Node::Text(r[..end].to_string()));
            }
            rest = &r[end + 3..];
        } else if rest.starts_with("<?") {
            rest = rest.find("?>").map_or("", |e| &rest[e + 2..]);
        } else if rest.starts_with("<!") {
            // A doctype: skip it, with any internal subset.
            let end = rest.find(']').filter(|&b| rest[..b].contains('[')).map_or(rest.find('>'), |b| rest[b..].find('>').map(|e| b + e));
            rest = end.map_or("", |e| &rest[e + 1..]);
        } else if let Some(r) = rest.strip_prefix("</") {
            let end = r.find('>').ok_or("unterminated end tag")?;
            let name = r[..end].trim();
            rest = &r[end + 1..];
            // Close up to the matching element (tolerates stray end tags).
            if let Some(pos) = stack.iter().rposition(|e| e.name == name).filter(|&p| p > 0) {
                while stack.len() > pos {
                    let e = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(Node::Element(e));
                }
            }
        } else {
            let (el, self_closing, after) = start_tag(&rest[1..])?;
            rest = after;
            if self_closing {
                stack.last_mut().unwrap().children.push(Node::Element(el));
            } else {
                stack.push(el);
            }
        }
    }
    while stack.len() > 1 {
        let e = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Element(e));
    }
    stack.pop().unwrap().elements().next().cloned().ok_or_else(|| "no root element".to_string())
}

fn push_text(stack: &mut [Element], raw: &str) {
    if let Some(top) = stack.last_mut() {
        top.children.push(Node::Text(unescape(raw)));
    }
}

/// Parses a start tag after its '<': (element, self-closing, rest of input).
fn start_tag(s: &str) -> Result<(Element, bool, &str), String> {
    let name_end = s.find(|c: char| c.is_whitespace() || c == '>' || c == '/').ok_or("unterminated start tag")?;
    let mut el = Element { name: s[..name_end].to_string(), ..Default::default() };
    let mut rest = &s[name_end..];
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("/>") {
            return Ok((el, true, r));
        }
        if let Some(r) = rest.strip_prefix('>') {
            return Ok((el, false, r));
        }
        if rest.is_empty() {
            return Err("unterminated start tag".into());
        }
        let eq = rest.find('=').ok_or("attribute without a value")?;
        let key = rest[..eq].trim().to_string();
        rest = rest[eq + 1..].trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or("unquoted attribute")?;
        let end = rest[1..].find(quote).ok_or("unterminated attribute")?;
        el.attrs.push((key, unescape(&rest[1..1 + end])));
        rest = &rest[end + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_attributes_and_text() {
        let doc = parse(
            r#"<?xml version="1.0"?><!-- feed --><feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/">
            <title>Books &amp; more</title>
            <entry><title><![CDATA[Pride & Prejudice]]></title><dc:language>en</dc:language>
            <link rel="http://opds-spec.org/acquisition" href="/b/1.epub" type="application/epub+zip"/></entry>
            </feed>"#,
        )
        .unwrap();
        assert_eq!(doc.local(), "feed");
        assert_eq!(doc.child_text("title").as_deref(), Some("Books & more"));
        let entry = doc.first("entry").unwrap();
        assert_eq!(entry.child_text("title").as_deref(), Some("Pride & Prejudice"));
        assert_eq!(entry.child_text("language").as_deref(), Some("en"));
        assert_eq!(entry.first("link").and_then(|l| l.attr("href")), Some("/b/1.epub"));
    }

    #[test]
    fn inline_markup_stays_inside_words() {
        let doc = parse(r#"<summary type="xhtml"><div>See https://<wbr/>en.wikipedia.org/<wbr/>wiki and <em>this</em>.</div><p>Next</p></summary>"#).unwrap();
        assert_eq!(doc.text(), "See https://en.wikipedia.org/wiki and this. Next");
    }

    #[test]
    fn character_references() {
        assert_eq!(unescape("caf&#233; &#x2014; &lt;b&gt; & more"), "café — <b> & more");
    }
}
