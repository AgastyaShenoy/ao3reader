//! Looking up a word: Wiktionary's definitions (free, no key; requests name the app, as
//! Wikimedia asks).

use serde_json::Value;
use std::time::Duration;
use ureq::Agent;

/// What a word means as one part of speech.
#[derive(Debug, Clone, PartialEq)]
pub struct Meaning {
    /// "Noun", "Verb", …
    pub part: String,
    pub definitions: Vec<String>,
}

/// Plain text from Wiktionary's bits of HTML (without the stylesheets some of them carry).
fn plain(html: &str) -> String {
    let mut html = html.to_string();
    for tag in ["style", "script"] {
        while let Some(a) = html.find(&format!("<{tag}")) {
            let b = html[a..].find(&format!("</{tag}>")).map_or(html.len(), |b| a + b + tag.len() + 3);
            html.replace_range(a..b, " ");
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    super::xml::unescape(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// English meanings from Wiktionary's definition data.
pub fn parse(json: &str) -> Vec<Meaning> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    v.get("en")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .map(|p| Meaning {
                    part: p.get("partOfSpeech").and_then(Value::as_str).unwrap_or_default().to_string(),
                    definitions: p
                        .get("definitions")
                        .and_then(Value::as_array)
                        .map(|ds| ds.iter().filter_map(|d| d.get("definition").and_then(Value::as_str)).map(plain).filter(|d| !d.is_empty()).collect())
                        .unwrap_or_default(),
                })
                .filter(|m| !m.definitions.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The word to look up from a selection: trimmed of punctuation around it.
pub fn word_of(selection: &str) -> String {
    selection.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'').to_string()
}

/// Meanings of `word` (its lower-case form too, for a capitalised word at a sentence start).
pub fn define(word: &str) -> Result<Vec<Meaning>, String> {
    let agent: Agent = Agent::config_builder()
        .user_agent(super::user_agent())
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut tried = Vec::new();
    for w in [word.to_string(), word.to_lowercase()] {
        if w.is_empty() || tried.contains(&w) {
            continue;
        }
        let url = format!("https://en.wiktionary.org/api/rest_v1/page/definition/{}", super::opds::encode(&w).replace('+', "%20"));
        tried.push(w);
        let mut resp = agent.get(&url).call().map_err(|e| format!("Couldn't reach Wiktionary: {e}"))?;
        if resp.status().as_u16() == 404 {
            continue;
        }
        let body = resp.body_mut().with_config().limit(4 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
        let meanings = parse(&body);
        if !meanings.is_empty() {
            return Ok(meanings);
        }
    }
    Err(format!("No definition found for “{word}”."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_meanings_in_plain_text() {
        let json = r#"{"en":[{"partOfSpeech":"Noun","language":"English","definitions":[{"definition":"A <a href=\"/wiki/plant\">plant</a> with blue flowers &amp; fibres.","examples":[]},{"definition":""}]}],
                      "fr":[{"partOfSpeech":"Nom","definitions":[{"definition":"lin"}]}]}"#;
        let m = parse(json);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].part, "Noun");
        assert_eq!(m[0].definitions, ["A plant with blue flowers & fibres."]);
        assert_eq!(word_of(" “Flax,” "), "Flax");
        let styled = r#"{"en":[{"partOfSpeech":"Determiner","definitions":[{"definition":"Each of two; both.<style data-mw-deduplicate=\"x\">.mw-parser-output .defdate{font-size:smaller}</style> <span class=\"defdate\">[from 9th c.]</span>"}]}]}"#;
        assert_eq!(parse(styled)[0].definitions, ["Each of two; both. [from 9th c.]"]);
    }
}
