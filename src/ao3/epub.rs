//! EPUB import.
//!
//! Reads any EPUB 2/3 book: chapters follow the book's own table of contents (so a file holding
//! several chapters is split at their anchors), with text extracted the same way as AO3 pages.
//! Headings, lists, tables, preformatted and centred text become tagged paragraphs (see
//! `rich_text::tagged`), and pictures are kept as files next to the saved book.
//! EPUBs downloaded from AO3 additionally give the work id, rating, warnings, fandoms,
//! relationships, characters, tags, stats, summary and per-chapter notes from their preface.

use crate::ao3::models::*;
use crate::ao3::parser::Ao3Parser;
use crate::reader::rich_text::{paragraph_words, tagged, FIELD};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

/// Refuse to inflate any single file beyond this (protects against zip bombs).
const MAX_ENTRY: usize = 64 * 1024 * 1024;
/// Pictures kept from one book, in total; later ones show their description instead.
const MAX_IMAGES: usize = 48 * 1024 * 1024;

struct Entry {
    offset: usize,
    method: u16,
    compressed: usize,
}

/// Minimal ZIP reader (stored and deflate entries), enough for EPUB containers.
struct Zip<'a> {
    data: &'a [u8],
    entries: HashMap<String, Entry>,
}

fn u16_at(d: &[u8], i: usize) -> Option<usize> {
    d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
}

fn u32_at(d: &[u8], i: usize) -> Option<usize> {
    d.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

impl<'a> Zip<'a> {
    fn open(data: &'a [u8]) -> Result<Self, String> {
        let bad = || "This file isn't a readable EPUB (damaged ZIP container).".to_string();
        let min = data.len().saturating_sub(22 + 65_535);
        let eocd = (min..=data.len().saturating_sub(22)).rev().find(|&i| u32_at(data, i) == Some(0x0605_4b50)).ok_or_else(bad)?;
        let count = u16_at(data, eocd + 10).ok_or_else(bad)?;
        let mut p = u32_at(data, eocd + 16).ok_or_else(bad)?;
        let mut entries = HashMap::new();
        for _ in 0..count {
            if u32_at(data, p) != Some(0x0201_4b50) {
                return Err(bad());
            }
            let method = u16_at(data, p + 10).ok_or_else(bad)? as u16;
            let compressed = u32_at(data, p + 20).ok_or_else(bad)?;
            let (n, e, c) = (u16_at(data, p + 28).ok_or_else(bad)?, u16_at(data, p + 30).ok_or_else(bad)?, u16_at(data, p + 32).ok_or_else(bad)?);
            let offset = u32_at(data, p + 42).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + n).ok_or_else(bad)?).into_owned();
            entries.insert(name, Entry { offset, method, compressed });
            p += 46 + n + e + c;
        }
        Ok(Self { data, entries })
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.entries.get(name)?;
        if u32_at(self.data, e.offset)? != 0x0403_4b50 {
            return None;
        }
        let start = e.offset + 30 + u16_at(self.data, e.offset + 26)? + u16_at(self.data, e.offset + 28)?;
        let raw = self.data.get(start..start + e.compressed)?;
        match e.method {
            0 => Some(raw.to_vec()),
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(raw).take(MAX_ENTRY as u64).read_to_end(&mut out).ok()?;
                Some(out)
            }
            _ => None,
        }
    }

    fn text(&self, name: &str) -> Option<String> {
        self.read(name).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            // Bytes, not a str slice: what follows a stray % may be a multi-byte character.
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves `href` (relative to `base`, a file inside the archive) to an archive path and fragment.
fn resolve(base: &str, href: &str) -> (String, Option<String>) {
    let (path, frag) = match href.split_once('#') {
        Some((p, f)) => (p, Some(percent_decode(f))),
        None => (href, None),
    };
    let path = percent_decode(path.split('?').next().unwrap_or(""));
    if path.is_empty() {
        return (base.to_string(), frag);
    }
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    (parts.join("/"), frag)
}

/// XHTML closes empty elements with "/>" (`<a id="x"/>`), which an HTML parser reads as an
/// opening tag: everything after an `<a/>` would end up inside the link, and a whole chapter
/// would read as one paragraph. Spelled out as `<a id="x"></a>`; the empty elements HTML knows
/// (`<br/>`, `<img/>`) stay as they are.
fn expand_self_closing(html: &str) -> Cow<'_, str> {
    const VOID: [&str; 14] = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r"<([A-Za-z][A-Za-z0-9:_-]*)(\s[^<>]*?)?\s*/>").expect("valid pattern"));
    tag.replace_all(html, |c: &regex::Captures| {
        let name = &c[1];
        if VOID.contains(&name.to_ascii_lowercase().as_str()) {
            c[0].to_string()
        } else {
            format!("<{name}{}></{name}>", c.get(2).map_or("", |m| m.as_str()))
        }
    })
}

fn elements(doc: &Html) -> impl Iterator<Item = ElementRef<'_>> {
    doc.root_element().descendants().filter_map(ElementRef::wrap)
}

fn text_of(e: ElementRef) -> String {
    e.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strips markup from a description that may itself contain escaped HTML.
fn plain(html: &str) -> String {
    let doc = Html::parse_fragment(html);
    let t = text_of(doc.root_element());
    t.trim().to_string()
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

struct Builder<'z> {
    chapters: Vec<Chapter>,
    /// A chapter was started by the table of contents (rather than by default).
    titled: bool,
    zip: &'z Zip<'z>,
    /// The document being read; pictures are found relative to it.
    file: String,
    /// Pictures already kept: archive path → file name.
    images: HashMap<String, String>,
    resources: Vec<(String, Vec<u8>)>,
    resource_bytes: usize,
}

/// File extension for picture data the reader can show, recognised by its first bytes.
fn image_ext(b: &[u8]) -> Option<&'static str> {
    match b {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'B', b'M', ..] => Some("bmp"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => None,
    }
}

impl Builder<'_> {
    /// Keeps a picture from the book (once, however often it appears) and adds it where it is.
    fn image(&mut self, src: &str, alt: &str) {
        let (path, _) = resolve(&self.file, src);
        let name = match self.images.get(&path) {
            Some(n) => n.clone(),
            None => {
                let Some(bytes) = self.zip.read(&path) else { return };
                let Some(ext) = image_ext(&bytes) else { return };
                if self.resource_bytes + bytes.len() > MAX_IMAGES {
                    return;
                }
                let name = format!("{}.{ext}", self.resources.len() + 1);
                self.resource_bytes += bytes.len();
                self.resources.push((name.clone(), bytes));
                self.images.insert(path, name.clone());
                name
            }
        };
        let alt = Ao3Parser::collapse_whitespace(&alt.replace(['\n', FIELD], " "));
        self.push(tagged("img", &format!("{name}{FIELD}{}", alt.trim())));
    }

    fn start(&mut self, title: String) {
        if let Some(c) = self.chapters.last_mut() {
            if c.paragraphs.is_empty() {
                // Nothing was read since the last start: just rename it.
                c.title = title;
                self.titled = true;
                return;
            }
        }
        let index = self.chapters.len() + 1;
        self.chapters.push(Chapter::new(index, title));
        self.titled = true;
    }

    fn push(&mut self, p: String) {
        if self.chapters.is_empty() {
            self.chapters.push(Chapter::new(1, String::new()));
            self.titled = false;
        }
        self.chapters.last_mut().unwrap().paragraphs.push(p);
    }
}

const BLOCKS: [&str; 14] = ["p", "blockquote", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "pre", "table", "center"];
const INLINE: [&str; 20] = [
    "a", "span", "em", "i", "b", "strong", "s", "strike", "del", "u", "small", "sup", "sub", "br", "abbr", "cite", "q", "code", "font", "mark",
];

/// Ends a run of loose inline text (text not inside a paragraph) as one paragraph.
fn flush(run: &mut String, b: &mut Builder) {
    let t = Ao3Parser::collapse_whitespace(run);
    let t = t.trim();
    if t.chars().count() > 1 {
        b.push(t.to_string());
    }
    run.clear();
}

/// Walks a document in order, splitting chapters at table-of-contents anchors.
fn walk(e: ElementRef, anchors: &HashMap<String, String>, b: &mut Builder) {
    let anchor = |el: ElementRef| el.value().id().or_else(|| el.value().attr("name")).and_then(|id| anchors.get(id)).cloned();
    let mut run = String::new();
    for child in e.children() {
        if let Some(el) = ElementRef::wrap(child) {
            let name = el.value().name();
            match name {
                "script" | "style" | "head" | "title" => continue,
                "img" => {
                    flush(&mut run, b);
                    if let Some(src) = el.value().attr("src") {
                        b.image(src, el.value().attr("alt").unwrap_or(""));
                    }
                    continue;
                }
                "svg" => {
                    // Cover pages often wrap their picture in an SVG <image>.
                    flush(&mut run, b);
                    for img in el.descendants().filter_map(ElementRef::wrap).filter(|d| d.value().name() == "image") {
                        // `xlink:href` is namespaced, so `attr("href")` doesn't see it.
                        if let Some((_, src)) = img.value().attrs().find(|(k, _)| *k == "href") {
                            b.image(src, "");
                        }
                    }
                    continue;
                }
                _ => {}
            }
            if let Some(label) = anchor(el) {
                flush(&mut run, b);
                b.start(label);
            }
            if INLINE.contains(&name) {
                // Loose inline text joins the surrounding run instead of becoming its own paragraph.
                if name == "br" {
                    run.push('\n');
                } else {
                    run.push_str(&Ao3Parser::clean_element_text(el));
                }
                continue;
            }
            flush(&mut run, b);
            if BLOCKS.contains(&name) {
                // An anchor inside a heading still starts the chapter before the heading.
                for d in el.descendants().skip(1).filter_map(ElementRef::wrap) {
                    if let Some(label) = anchor(d) {
                        b.start(label);
                    }
                }
                block(el, b);
            } else {
                walk(el, anchors, b);
            }
        } else if let Some(t) = child.value().as_text() {
            run.extend(t.chars().map(|c| if c.is_ascii_whitespace() { ' ' } else { c }));
        }
    }
    flush(&mut run, b);
}

/// Centred by its `align` attribute, inline style or a class name such as "center".
fn centered(el: ElementRef) -> bool {
    let v = el.value();
    v.attr("align").is_some_and(|a| a.eq_ignore_ascii_case("center"))
        || v.attr("style").is_some_and(|s| s.to_ascii_lowercase().replace(' ', "").contains("text-align:center"))
        || v.classes().any(|c| {
            let c = c.to_ascii_lowercase();
            c.contains("center") || c.contains("centre")
        })
}

/// One tagged paragraph per list item; nested lists go one level deeper.
fn list(el: ElementRef, depth: u8, b: &mut Builder) {
    let ordered = el.value().name() == "ol";
    let mut n: u32 = el.value().attr("start").and_then(|s| s.trim().parse().ok()).unwrap_or(1);
    for li in el.children().filter_map(ElementRef::wrap).filter(|c| c.value().name() == "li") {
        let text = Ao3Parser::collapse_whitespace(&Ao3Parser::clean_element_text_skipping(li, &["ul", "ol"]));
        let text = text.trim();
        if !text.is_empty() {
            let tag = if ordered { format!("ol{depth}.{n}") } else { format!("ul{depth}") };
            b.push(tagged(&tag, text));
        }
        n += 1;
        for sub in li.descendants().filter_map(ElementRef::wrap).filter(|d| matches!(d.value().name(), "ul" | "ol")) {
            // Only lists directly inside this item; deeper ones are handled by their own parent.
            let parent_li = sub.ancestors().filter_map(ElementRef::wrap).find(|a| a.value().name() == "li").map(|a| a.id());
            if parent_li == Some(li.id()) {
                list(sub, (depth + 1).min(8), b);
            }
        }
    }
}

/// One tagged paragraph per table row, cells separated by `FIELD`.
fn table(el: ElementRef, b: &mut Builder) {
    for tr in el.descendants().filter_map(ElementRef::wrap).filter(|d| d.value().name() == "tr") {
        // Rows of a table nested inside a cell are part of that cell's text.
        let owner = tr.ancestors().filter_map(ElementRef::wrap).find(|a| a.value().name() == "table").map(|t| t.id());
        if owner != Some(el.id()) {
            continue;
        }
        let cells: Vec<ElementRef> = tr.children().filter_map(ElementRef::wrap).filter(|c| matches!(c.value().name(), "td" | "th")).collect();
        let texts: Vec<String> = cells
            .iter()
            .map(|c| Ao3Parser::collapse_whitespace(&Ao3Parser::clean_element_text(*c)).replace(['\n', FIELD], " ").trim().to_string())
            .collect();
        if texts.iter().all(|t| t.is_empty()) {
            continue;
        }
        let header = cells.iter().all(|c| c.value().name() == "th");
        b.push(tagged(if header { "th" } else { "tr" }, &texts.join(&FIELD.to_string())));
    }
}

fn block(el: ElementRef, b: &mut Builder) {
    let name = el.value().name();
    match name {
        "hr" => {
            b.push("---".into());
            return;
        }
        "ul" | "ol" => return list(el, 1, b),
        "table" => return table(el, b),
        "pre" => {
            let raw: String = el.text().collect();
            let raw = raw.replace("\r\n", "\n");
            let raw = raw.trim_matches('\n').trim_end();
            if !raw.trim().is_empty() {
                b.push(tagged("pre", raw));
            }
            return;
        }
        _ => {}
    }
    // Pictures inside a paragraph or heading come before its text.
    for img in el.descendants().filter_map(ElementRef::wrap).filter(|d| d.value().name() == "img") {
        if let Some(src) = img.value().attr("src") {
            b.image(src, img.value().attr("alt").unwrap_or(""));
        }
    }
    let text = Ao3Parser::collapse_whitespace(&Ao3Parser::clean_element_text(el));
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if name == "blockquote" {
        b.push(format!("> {}", text.replace('\n', "\n> ")));
        return;
    }
    if name.len() == 2 && name.starts_with('h') {
        if let Some(c) = b.chapters.last_mut() {
            if c.paragraphs.is_empty() {
                // Headings before any text are the chapter's heading, not body text; they name
                // the chapter when the table of contents didn't.
                if !b.titled || c.title.is_empty() {
                    c.title = text.replace('*', "");
                    b.titled = true;
                }
                return;
            }
        } else {
            b.chapters.push(Chapter::new(1, text.replace('*', "")));
            b.titled = true;
            return;
        }
        b.push(tagged(name, text.trim_matches('*').trim()));
        return;
    }
    if name == "center" || centered(el) {
        b.push(tagged("c", text));
        return;
    }
    b.push(text.to_string());
}

/// AO3 chapter files keep summaries and notes as a label followed by a quote.
fn pull_note(paras: &mut Vec<String>, labels: &[&str]) -> Option<String> {
    let i = paras.iter().position(|p| labels.contains(&norm(p).as_str()))?;
    let mut j = i + 1;
    let mut parts = Vec::new();
    while j < paras.len() && paras[j].starts_with("> ") {
        parts.push(paras[j].lines().map(|l| l.strip_prefix("> ").unwrap_or(l)).collect::<Vec<_>>().join("\n"));
        j += 1;
    }
    if parts.is_empty() {
        return None;
    }
    paras.drain(i..j);
    Some(parts.join("\n\n"))
}

/// Metadata from AO3's preface: `<dt>Label:</dt><dd>values</dd>` pairs.
fn ao3_preface(doc: &Html, meta: &mut WorkMeta) -> bool {
    let dt = Selector::parse("dt").unwrap();
    let mut found = false;
    for d in doc.select(&dt) {
        let label = norm(&text_of(d));
        let Some(dd) = d.next_siblings().filter_map(ElementRef::wrap).find(|e| e.value().name() == "dd") else { continue };
        let links: Vec<String> = dd.select(&Selector::parse("a").unwrap()).map(text_of).filter(|t| !t.is_empty()).collect();
        let values = if links.is_empty() { text_of(dd).split(", ").map(str::to_string).filter(|s| !s.is_empty()).collect() } else { links };
        match label.as_str() {
            "rating" | "ratings" => {
                meta.rating = WorkRating::from_str_loose(&values.join(" "));
                found = true;
            }
            "archivewarning" | "archivewarnings" => meta.warnings = values,
            "category" | "categories" => meta.categories = values.iter().map(|v| Category::from_str_loose(v)).collect(),
            "fandom" | "fandoms" => {
                meta.fandoms = values;
                found = true;
            }
            "relationship" | "relationships" => meta.relationships = values,
            "character" | "characters" => meta.characters = values,
            "additionaltags" | "freeforms" => meta.freeform_tags = values,
            "language" => meta.language = text_of(dd),
            "stats" => {
                let s = text_of(dd);
                let num = |re: &str| Regex::new(re).ok().and_then(|r| r.captures(&s)).map(|c| c[1].replace(',', ""));
                if let Some(w) = num(r"Words:\s*([\d,]+)") {
                    meta.stats.words = w.parse().unwrap_or(0);
                }
                if let Some(p) = num(r"Published:\s*([\d-]+)") {
                    meta.stats.published_date = p;
                }
                if let Some(c) = Regex::new(r"Chapters:\s*(\d+)/(\d+|\?)").ok().and_then(|r| r.captures(&s)) {
                    meta.stats.chapters_published = c[1].parse().unwrap_or(0);
                    meta.stats.chapters_total = c[2].parse().ok();
                    meta.is_complete = meta.stats.chapters_total == Some(meta.stats.chapters_published);
                }
            }
            _ => {}
        }
    }
    if !found {
        return false;
    }
    if let Some(h) = doc.select(&Selector::parse("h1").unwrap()).next() {
        let t = text_of(h);
        if !t.is_empty() {
            meta.title = t;
        }
    }
    let authors: Vec<String> = doc.select(&Selector::parse("a[rel=author]").unwrap()).map(text_of).filter(|t| !t.is_empty()).collect();
    if !authors.is_empty() {
        meta.authors = authors;
    }
    // "Summary" label followed by a quote.
    let mut paras = Ao3Parser::extract_paragraphs(doc.root_element());
    if let Some(s) = pull_note(&mut paras, &["summary"]) {
        meta.summary_text = s;
    }
    true
}

pub fn parse_epub(bytes: &[u8]) -> Result<Work, String> {
    let zip = Zip::open(bytes)?;
    let container = zip.text("META-INF/container.xml").ok_or("This EPUB has no META-INF/container.xml.")?;
    let opf_path = elements(&Html::parse_document(&container))
        .find(|e| e.value().name() == "rootfile")
        .and_then(|e| e.value().attr("full-path"))
        .map(str::to_string)
        .ok_or("This EPUB doesn't say where its package file is.")?;
    let opf_text = zip.text(&opf_path).ok_or("This EPUB's package file is missing.")?;
    let opf = Html::parse_document(&opf_text);

    let mut meta = WorkMeta::default();
    let first = |name: &str| elements(&opf).find(|e| e.value().name() == name).map(text_of).filter(|t| !t.is_empty());
    if let Some(t) = first("dc:title") {
        meta.title = t;
    }
    let creators: Vec<String> = elements(&opf).filter(|e| e.value().name() == "dc:creator").map(text_of).filter(|t| !t.is_empty()).collect();
    if !creators.is_empty() {
        meta.authors = creators;
    }
    if let Some(d) = first("dc:description") {
        meta.summary_text = plain(&d);
    }
    if let Some(l) = first("dc:language") {
        meta.language = l;
    }
    meta.freeform_tags = elements(&opf).filter(|e| e.value().name() == "dc:subject").map(text_of).filter(|t| !t.is_empty()).collect();

    // Manifest and reading order.
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
    for e in elements(&opf).filter(|e| e.value().name() == "item") {
        let v = e.value();
        if let (Some(id), Some(href)) = (v.attr("id"), v.attr("href")) {
            let path = resolve(&opf_path, href).0;
            manifest.insert(id.to_string(), (path, v.attr("media-type").unwrap_or("").to_string(), v.attr("properties").unwrap_or("").to_string()));
        }
    }
    let spine: Vec<String> = elements(&opf)
        .filter(|e| e.value().name() == "itemref")
        .filter_map(|e| e.value().attr("idref"))
        .filter_map(|id| manifest.get(id))
        .filter(|(_, mt, _)| mt.contains("html") || mt.is_empty())
        .map(|(p, _, _)| p.clone())
        .collect();
    if spine.is_empty() {
        return Err("This EPUB has no readable text documents.".into());
    }

    // Table of contents: NCX (EPUB 2) or the navigation document (EPUB 3).
    let mut toc: Vec<(String, Option<String>, String)> = Vec::new();
    let ncx = manifest.values().find(|(_, mt, _)| mt == "application/x-dtbncx+xml").map(|(p, _, _)| p.clone());
    if let Some(ncx_text) = ncx.as_deref().and_then(|p| zip.text(p)) {
        let doc = Html::parse_document(&ncx_text);
        let mut label = String::new();
        for e in elements(&doc) {
            match e.value().name() {
                // Page lists and other nav lists follow the table of contents; they aren't chapters.
                "pagelist" | "navlist" => break,
                "text" => label = text_of(e),
                "content" => {
                    if let Some(src) = e.value().attr("src") {
                        let (path, frag) = resolve(ncx.as_deref().unwrap_or(""), src);
                        toc.push((path, frag, std::mem::take(&mut label)));
                    }
                }
                _ => {}
            }
        }
    }
    if toc.is_empty() {
        if let Some((nav_path, _, _)) = manifest.values().find(|(_, _, props)| props.split_whitespace().any(|p| p == "nav")) {
            if let Some(nav_text) = zip.text(nav_path) {
                let doc = Html::parse_document(&nav_text);
                // The "toc" nav, not the page-list or landmarks navs.
                let navs: Vec<ElementRef> = doc.select(&Selector::parse("nav").unwrap()).collect();
                let toc_nav = navs.iter().find(|n| n.value().attr("epub:type").is_some_and(|t| t.contains("toc"))).or(navs.first());
                let links = Selector::parse("a[href]").unwrap();
                for a in toc_nav.into_iter().flat_map(|n| n.select(&links)) {
                    let (path, frag) = resolve(nav_path, a.value().attr("href").unwrap_or(""));
                    toc.push((path, frag, text_of(a)));
                }
            }
        }
    }
    let label = |l: &str| {
        let l = l.trim();
        if l.chars().count() > 120 { l.chars().take(117).collect::<String>() + "…" } else { l.to_string() }
    };

    let ao3_url = Regex::new(r"archiveofourown\.org/works/(\d+)").unwrap();
    // AO3's EPUBs name the work in their metadata. A link in the text alone doesn't make a book an
    // AO3 work (it might just mention one): that needs AO3's own preface or afterword too.
    let work_id = ao3_url.captures(&opf_text).map(|c| c[1].to_string());
    let mut linked_id = None;
    let mut is_ao3 = work_id.is_some();
    let mut b = Builder { chapters: Vec::new(), titled: false, zip: &zip, file: String::new(), images: HashMap::new(), resources: Vec::new(), resource_bytes: 0 };

    for path in &spine {
        let Some(html) = zip.text(path) else { continue };
        b.file = path.clone();
        if linked_id.is_none() {
            linked_id = ao3_url.captures(&html).map(|c| c[1].to_string());
        }
        let doc = Html::parse_document(&expand_self_closing(&html));
        // AO3's preface (metadata) and afterword are not chapters.
        if ao3_preface(&doc, &mut meta) {
            is_ao3 = true;
            continue;
        }
        if html.contains("Please drop by the Archive and comment") || html.contains("Please drop by the archive and comment") {
            is_ao3 = true;
            continue;
        }
        let anchors: HashMap<String, String> = toc.iter().filter(|(p, f, _)| p == path && f.is_some()).map(|(_, f, l)| (f.clone().unwrap(), label(l))).collect();
        if let Some((_, _, l)) = toc.iter().find(|(p, f, _)| p == path && f.is_none()) {
            b.start(label(l));
        } else if toc.is_empty() {
            b.start(String::new());
            b.titled = false;
        }
        let body = Selector::parse("body").unwrap();
        let root = doc.select(&body).next().unwrap_or(doc.root_element());
        walk(root, &anchors, &mut b);
    }

    let resources = std::mem::take(&mut b.resources);
    let mut chapters: Vec<Chapter> = b.chapters.into_iter().filter(|c| !c.paragraphs.is_empty()).collect();
    if chapters.is_empty() {
        return Err("No readable text was found in this EPUB.".into());
    }
    for (i, c) in chapters.iter_mut().enumerate() {
        c.index = i + 1;
        if is_ao3 {
            c.paragraphs.retain(|p| !p.contains("See the end of the chapter for") && !p.contains("See the end of the work for"));
            c.summary = pull_note(&mut c.paragraphs, &["summary", "chaptersummary"]);
            c.pre_notes = pull_note(&mut c.paragraphs, &["notes", "chapternotes", "authorsnote", "authorsnotes"]);
            c.post_notes = pull_note(&mut c.paragraphs, &["endnotes", "chapterendnotes"]);
        }
        if c.title.trim().is_empty() {
            c.title = format!("Chapter {}", i + 1);
        }
        c.word_count = c.paragraphs.iter().map(|p| paragraph_words(p)).sum();
    }
    if meta.stats.words == 0 {
        meta.stats.words = chapters.iter().map(|c| c.word_count).sum();
    }
    if meta.stats.chapters_published == 0 {
        meta.stats.chapters_published = chapters.len() as u32;
        meta.stats.chapters_total = Some(chapters.len() as u32);
        meta.is_complete = true;
    }
    meta.id = work_id.or(linked_id.filter(|_| is_ao3)).unwrap_or_else(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        format!("epub-{:x}", h.finish())
    });
    Ok(Work { meta, chapters, fetched_at: chrono::Utc::now(), resources })
}

/// Imports a saved work from file contents, recognised by what's inside: an EPUB (ZIP), a PDF,
/// a work saved by Flax Reader (JSON), or an AO3 HTML page or download.
pub fn import_bytes(bytes: &[u8]) -> Result<Work, String> {
    import_named(bytes, None)
}

/// `import_bytes`, with the file's name (without extension) for books that don't give a title.
pub fn import_named(bytes: &[u8], name: Option<&str>) -> Result<Work, String> {
    #[cfg(feature = "pdf")]
    if crate::pdf::is_pdf(bytes) {
        return crate::pdf::import(bytes, name);
    }
    let _ = name;
    if bytes.starts_with(b"PK\x03\x04") {
        return parse_epub(bytes);
    }
    let text = String::from_utf8_lossy(bytes);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('{') {
        let mut work = serde_json::from_str::<Work>(t).map_err(|e| format!("Not a saved Flax Reader work: {e}"))?;
        if work.chapters.is_empty() {
            return Err("This saved work has no chapters to read.".into());
        }
        // The id names files on disk: keep it to letters, digits, - and _.
        let safe: String = work.meta.id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        work.meta.id = if safe.trim_matches('_').is_empty() { "imported".into() } else { safe };
        return Ok(work);
    }
    let id = Ao3Parser::extract_work_id(t).unwrap_or_else(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        t.hash(&mut h);
        format!("local-{:x}", h.finish())
    });
    Ao3Parser::parse_work(t, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an uncompressed ZIP in memory (test fixture only).
    fn stored_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let files: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_bytes())).collect();
        stored_zip_bytes(&files)
    }

    fn stored_zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, body) in files {
            let off = out.len() as u32;
            let header = |sig: u32, extra: &[u8]| {
                let mut h = sig.to_le_bytes().to_vec();
                h.extend_from_slice(extra);
                h
            };
            let mut local = header(0x0403_4b50, &[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            local.extend_from_slice(&(body.len() as u32).to_le_bytes());
            local.extend_from_slice(&(body.len() as u32).to_le_bytes());
            local.extend_from_slice(&(name.len() as u16).to_le_bytes());
            local.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&local);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(body);
            let mut c = header(0x0201_4b50, &[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.extend_from_slice(&(body.len() as u32).to_le_bytes());
            c.extend_from_slice(&(body.len() as u32).to_le_bytes());
            c.extend_from_slice(&(name.len() as u16).to_le_bytes());
            c.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.extend_from_slice(&off.to_le_bytes());
            c.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&c);
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn self_closing_tags_are_spelled_out() {
        let x = r#"<h1><span class="hide"><a id="t7"/>Title</span></h1><p>One.<br/>Two</p><img src="a.jpg" /><div class="x" />"#;
        assert_eq!(expand_self_closing(x), r#"<h1><span class="hide"><a id="t7"></a>Title</span></h1><p>One.<br/>Two</p><img src="a.jpg" /><div class="x"></div>"#);
        // Parsed, the paragraphs stay paragraphs instead of running into the link.
        let doc = Html::parse_document(&expand_self_closing(r#"<body><h1><a id="t7"/>Title</h1><p>One.</p><p>Two.</p></body>"#));
        let ps: Vec<String> = doc.select(&Selector::parse("p").unwrap()).map(|p| p.text().collect()).collect();
        assert_eq!(ps, ["One.", "Two."]);
        assert!(doc.select(&Selector::parse("a p").unwrap()).next().is_none());
    }

    const CONTAINER: &str = r#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    #[test]
    fn ao3_style_epub() {
        let opf = r#"<?xml version="1.0"?><package><metadata><dc:title>Starlit</dc:title><dc:creator>Writer</dc:creator></metadata>
            <manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
            <item id="p" href="Starlit_split_000.xhtml" media-type="application/xhtml+xml"/>
            <item id="c1" href="Starlit_split_001.xhtml" media-type="application/xhtml+xml"/>
            <item id="a" href="Starlit_split_002.xhtml" media-type="application/xhtml+xml"/></manifest>
            <spine toc="ncx"><itemref idref="p"/><itemref idref="c1"/><itemref idref="a"/></spine></package>"#;
        let ncx = r#"<ncx><navMap><navPoint><navLabel><text>Preface</text></navLabel><content src="Starlit_split_000.xhtml"/></navPoint>
            <navPoint><navLabel><text>1. The Library</text></navLabel><content src="Starlit_split_001.xhtml"/></navPoint></navMap></ncx>"#;
        let preface = r##"<html><body><div id="preface"><p class="message">Posted originally on the <a href="http://archiveofourown.org/">Archive of Our Own</a> at <a href="http://archiveofourown.org/works/424242">http://archiveofourown.org/works/424242</a>.</p>
            <div class="meta"><dl class="tags"><dt>Rating:</dt><dd><a href="#">Teen And Up Audiences</a></dd>
            <dt>Archive Warning:</dt><dd><a href="#">No Archive Warnings Apply</a></dd><dt>Category:</dt><dd><a href="#">F/F</a></dd>
            <dt>Fandom:</dt><dd><a href="#">Original Work</a></dd><dt>Relationship:</dt><dd><a href="#">A/B</a></dd>
            <dt>Stats:</dt><dd>Published: 2024-01-02 Words: 1,234 Chapters: 1/1</dd></dl>
            <h1>Starlit Library</h1><div class="byline">by <a rel="author" href="#">Writer Person</a></div>
            <p>Summary</p><blockquote class="userstuff"><p>Two archivists meet.</p></blockquote></div></div></body></html>"##;
        let chapter = r#"<html><body><div class="meta group"><h2 class="heading">The Library</h2><p>Chapter Notes</p><blockquote><p>Thanks for reading!</p></blockquote></div>
            <div class="userstuff"><p>It was <em>quiet</em> in the stacks.</p><p>She turned a page.</p></div></body></html>"#;
        let after = r#"<html><body><div id="afterword"><p>Please drop by the Archive and comment to let the creator know if you enjoyed their work!</p></div></body></html>"#;
        let bytes = stored_zip(&[
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", opf),
            ("OEBPS/toc.ncx", ncx),
            ("OEBPS/Starlit_split_000.xhtml", preface),
            ("OEBPS/Starlit_split_001.xhtml", chapter),
            ("OEBPS/Starlit_split_002.xhtml", after),
        ]);
        let w = import_bytes(&bytes).unwrap();
        assert_eq!(w.meta.id, "424242");
        assert_eq!(w.meta.title, "Starlit Library");
        assert_eq!(w.meta.authors, vec!["Writer Person".to_string()]);
        assert_eq!(w.meta.rating, WorkRating::Teen);
        assert_eq!(w.meta.fandoms, vec!["Original Work".to_string()]);
        assert_eq!(w.meta.relationships, vec!["A/B".to_string()]);
        assert_eq!(w.meta.categories, vec![Category::FF]);
        assert_eq!(w.meta.stats.words, 1234);
        assert!(w.meta.is_complete);
        assert_eq!(w.meta.summary_text, "Two archivists meet.");
        assert_eq!(w.chapters.len(), 1);
        let c = &w.chapters[0];
        assert_eq!(c.title, "1. The Library");
        assert_eq!(c.pre_notes.as_deref(), Some("Thanks for reading!"));
        assert_eq!(c.paragraphs, vec!["It was *quiet* in the stacks.".to_string(), "She turned a page.".to_string()]);
    }

    #[test]
    fn book_split_by_toc_anchors() {
        let opf = r#"<package><metadata><dc:title>Two Tales</dc:title><dc:creator>Anon</dc:creator><dc:description>&lt;p&gt;Short &amp; sweet.&lt;/p&gt;</dc:description></metadata>
            <manifest><item id="t" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/><item id="b" href="text/book.xhtml" media-type="application/xhtml+xml"/></manifest>
            <spine><itemref idref="b"/></spine></package>"#;
        let nav = r#"<html><body><nav><ol><li><a href="text/book.xhtml#c1">First Tale</a></li><li><a href="text/book.xhtml#c2">Second Tale</a></li></ol></nav></body></html>"#;
        let book = r#"<html><body><h1 id="c1">I</h1><p>Once upon a time.</p><hr/><div>The end of <a href="x">one</a>.</div><h2><a id="c2"></a>II</h2><p>Another time.</p></body></html>"#;
        let bytes = stored_zip(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", opf),
            ("OEBPS/nav.xhtml", nav),
            ("OEBPS/text/book.xhtml", book),
        ]);
        let w = parse_epub(&bytes).unwrap();
        assert!(w.meta.id.starts_with("epub-"));
        assert_eq!(w.meta.summary_text, "Short & sweet.");
        let titles: Vec<&str> = w.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["First Tale", "Second Tale"]);
        assert_eq!(w.chapters[0].paragraphs, vec!["Once upon a time.", "---", "The end of one."]);
        assert_eq!(w.chapters[1].paragraphs, vec!["Another time."]);
    }

    #[test]
    fn structure_and_pictures() {
        use crate::reader::rich_text::{image_source, paragraph_display, table_cells, ParagraphKind};
        let opf = r#"<package><metadata><dc:title>Shapes</dc:title></metadata>
            <manifest><item id="c" href="text/c.xhtml" media-type="application/xhtml+xml"/><item id="i" href="img/map.png" media-type="image/png"/></manifest>
            <spine><itemref idref="c"/></spine></package>"#;
        let body = r#"<html><body><h1>Shapes</h1><p>Intro with <em>style</em>.</p>
            <h2>Lists</h2><ul><li>One<ul><li>One-a</li></ul></li><li>Two</li></ul><ol start="4"><li>Four</li><li>Five</li></ol>
            <table><tr><th>Name</th><th>Age</th></tr><tr><td>Ann</td><td><b>30</b></td></tr></table>
            <pre>  line one
    line two</pre><p class="center">* * *centred*</p><p><img src="../img/map.png" alt="A map"/></p><img src="../img/map.png"/>
            <p><img src="../img/missing.png" alt="gone"/>Still here.</p></body></html>"#;
        let png: &[u8] = b"\x89PNG\r\n\x1a\nfake";
        let bytes = stored_zip_bytes(&[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/text/c.xhtml", body.as_bytes()),
            ("OEBPS/img/map.png", png),
        ]);
        let w = parse_epub(&bytes).unwrap();
        let p = &w.chapters[0].paragraphs;
        let kinds: Vec<ParagraphKind> = p.iter().map(|x| paragraph_display(x).0).collect();
        assert_eq!(
            kinds,
            [
                ParagraphKind::Text,
                ParagraphKind::Heading(2),
                ParagraphKind::Bullet(1),
                ParagraphKind::Bullet(2),
                ParagraphKind::Bullet(1),
                ParagraphKind::Numbered(1, 4),
                ParagraphKind::Numbered(1, 5),
                ParagraphKind::TableRow(true),
                ParagraphKind::TableRow(false),
                ParagraphKind::Pre,
                ParagraphKind::Center,
                ParagraphKind::Image,
                ParagraphKind::Image,
                ParagraphKind::Text,
            ]
        );
        assert_eq!(w.chapters[0].title, "Shapes");
        assert_eq!(paragraph_display(&p[3]).1.text, "One-a");
        assert_eq!(table_cells(&p[8]).iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["Ann", "30"]);
        assert_eq!(paragraph_display(&p[9]).1.text, "  line one\n    line two");
        // The same picture is kept once and used twice; a missing one is left out.
        assert_eq!(image_source(&p[11]), Some(("1.png", "A map")));
        assert_eq!(image_source(&p[12]), Some(("1.png", "")));
        assert_eq!(w.resources.len(), 1);
        assert_eq!(w.resources[0].1, png);
        assert_eq!(paragraph_display(&p[13]).1.text, "Still here.");
    }

    #[test]
    fn svg_cover_picture() {
        let opf = r#"<package><metadata><dc:title>Covered</dc:title></metadata>
            <manifest><item id="cv" href="cover.xhtml" media-type="application/xhtml+xml"/><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest>
            <spine><itemref idref="cv"/><itemref idref="c"/></spine></package>"#;
        let cover = r#"<html><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="cover.jpg"/></svg></body></html>"#;
        let jpg: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        let bytes = stored_zip_bytes(&[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/cover.xhtml", cover.as_bytes()),
            ("OEBPS/c.xhtml", b"<html><body><p>Text.</p></body></html>"),
            ("OEBPS/cover.jpg", jpg),
        ]);
        let w = parse_epub(&bytes).unwrap();
        assert_eq!(w.resources.len(), 1);
        assert_eq!(w.resources[0].0, "1.jpg");
        assert!(w.chapters[0].paragraphs[0].contains("1.jpg"));
    }

    #[test]
    fn import_recognises_json_and_rejects_garbage_zip() {
        assert!(import_bytes(b"PK\x03\x04not really a zip").is_err());
        let work = Work { meta: WorkMeta { id: "7".into(), ..Default::default() }, chapters: vec![Chapter::new(1, "One".into())], fetched_at: chrono::Utc::now(), resources: Vec::new() };
        let json = serde_json::to_vec(&work).unwrap();
        assert_eq!(import_bytes(&json).unwrap().meta.id, "7");
    }
}
