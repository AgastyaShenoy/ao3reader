//! Fetch screen: download a work by link or ID (to read now or save for later), download
//! AO3's EPUB, or import a saved .json/.html file.

use super::gfx::*;
use super::icons;
use super::reader::preface::{category_color, pills, rating_color, thousands};
use super::ui::*;
use crate::ao3::models::WorkMeta;
use crate::ao3::parser::Ao3Parser;

const MAX_W: f32 = 820.0;

pub enum FetchAction {
    Fetch { id: String, open: bool },
    Epub(String),
    Import,
    Open(String),
    GoAccount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Work,
    Epub,
    Import,
}

#[derive(Default)]
pub struct FetchScreen {
    input: String,
    pub busy: Option<Busy>,
    error: Option<String>,
    preview: Option<WorkMeta>,
}

impl FetchScreen {
    pub fn set_error(&mut self, e: String) {
        self.busy = None;
        self.error = Some(e);
    }

    pub fn set_done(&mut self, meta: Option<WorkMeta>) {
        self.busy = None;
        self.error = None;
        if meta.is_some() {
            self.preview = meta;
        }
    }

    pub fn frame(&mut self, ui: &mut Ui, body: Rect, logged_in: bool) -> Vec<FetchAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let w = (body.w - 48.0).min(MAX_W);
        let x = body.x + (body.w - w) / 2.0;
        let t = ui.text("Fetch a work", &TextStyle::ui(26.0).semibold(), x, body.y + 22.0, c.text);
        let top = body.y + 22.0 + t.h + 16.0;
        let area = Rect::new(body.x, top, body.w, (body.bottom() - top).max(0.0));
        ui.scroll_area(id("fetch-scroll"), area, |ui, _view, y0| {
            let mut col = Col::new(x, y0 + 2.0, w);
            self.content(ui, &mut col, logged_in, &mut out);
            col.y + 24.0 - y0
        });
        out
    }

    fn content(&mut self, ui: &mut Ui, col: &mut Col, logged_in: bool, out: &mut Vec<FetchAction>) {
        let c = ui.c;
        let w = col.w;

        // Input card.
        let card = col.take(176.0);
        col.gap(12.0);
        ui.card(card);
        ui.text("AO3 link or work ID", &TextStyle::ui(13.0).semibold(), card.x + 18.0, card.y + 16.0, c.text);
        let busy = self.busy.is_some();
        let input = Rect::new(card.x + 18.0, card.y + 42.0, card.w - 36.0 - 92.0, 38.0);
        let resp = ui.text_input(id("fetch-input"), input, &mut self.input, "https://archiveofourown.org/works/12345678", false);
        let iid = id("fetch-input");
        if ui.button_ex(id("fetch-paste"), Rect::new(input.right() + 8.0, input.y, 84.0, 38.0), "Paste", Some("\u{E77F}"), ButtonKind::Secondary, true) {
            if let Some(e) = ui.mem.edits.get(&iid) {
                e.paste();
            }
        }
        let work_id = Ao3Parser::extract_work_id(self.input.trim());
        let can = work_id.is_some() && !busy;
        let row_y = card.y + 96.0;
        let mut bx = card.x + 18.0;
        let mut button = |ui: &mut Ui, key: &str, label: &str, icon: &str, kind: ButtonKind, enabled: bool| {
            let bw = ui.button_width(label, Some(icon));
            let r = Rect::new(bx, row_y, bw, 36.0);
            bx += bw + 8.0;
            ui.button_ex(id(key), r, label, Some(icon), kind, enabled)
        };
        let read = button(ui, "fetch-read", "Download & read", icons::READING, ButtonKind::Primary, can);
        let save = button(ui, "fetch-save", "Save to library", icons::DOWNLOAD, ButtonKind::Secondary, can);
        let epub = button(ui, "fetch-epub", "EPUB", "\u{E8A5}", ButtonKind::Secondary, can);
        let iw = ui.button_width("Import file…", Some(icons::OPEN_FILE));
        let import = ui.button_ex(id("fetch-import"), Rect::new(card.right() - 18.0 - iw, row_y, iw, 36.0), "Import file…", Some(icons::OPEN_FILE), ButtonKind::Ghost, !busy);
        if let Some(wid) = work_id {
            if read || (resp.submitted && can) {
                self.start(Busy::Work);
                out.push(FetchAction::Fetch { id: wid, open: true });
            } else if save {
                self.start(Busy::Work);
                out.push(FetchAction::Fetch { id: wid, open: false });
            } else if epub {
                self.start(Busy::Epub);
                out.push(FetchAction::Epub(wid));
            }
        }
        if import {
            out.push(FetchAction::Import);
        }

        let status_y = card.y + 144.0;
        match self.busy {
            Some(b) => {
                ui.spinner(card.x + 28.0, status_y + 8.0, 7.0);
                let msg = match b {
                    Busy::Work => "Downloading and reading the work… long works can take a little while.",
                    Busy::Epub => "Downloading the EPUB from AO3…",
                    Busy::Import => "Importing the file…",
                };
                ui.text(msg, &TextStyle::ui(13.0), card.x + 44.0, status_y, c.muted);
            }
            None if !self.input.trim().is_empty() && Ao3Parser::extract_work_id(self.input.trim()).is_none() => {
                ui.text("That doesn't look like an AO3 work link or ID.", &TextStyle::ui(13.0), card.x + 18.0, status_y, c.danger);
            }
            None => {}
        }

        if let Some(err) = self.error.clone() {
            let restricted = err.to_lowercase().contains("restricted") || err.to_lowercase().contains("log in");
            let t = ui.measure(&err, &TextStyle::ui(13.5), TextOpts::wrap(w - 60.0));
            let h = t.h + if restricted && !logged_in { 92.0 } else { 52.0 };
            let r = col.take(h);
            col.gap(12.0);
            ui.g.fill_round(r, 8.0, c.danger.alpha(0.1));
            ui.g.stroke_round(r, 8.0, 1.0, c.danger.alpha(0.6));
            ui.text("Couldn't download", &TextStyle::ui(13.5).semibold(), r.x + 18.0, r.y + 14.0, c.danger);
            ui.g.draw_text(t, r.x + 18.0, r.y + 36.0, c.text);
            if restricted && !logged_in {
                let bw = ui.button_width("Sign in to AO3", Some(icons::CONTACT));
                if ui.button_ex(id("fetch-go-account"), Rect::new(r.x + 18.0, r.bottom() - 48.0, bw, 34.0), "Sign in to AO3", Some(icons::CONTACT), ButtonKind::Secondary, true) {
                    out.push(FetchAction::GoAccount);
                }
            }
        }

        if let Some(m) = self.preview.clone() {
            let summary = m.summary_text.split_whitespace().collect::<Vec<_>>().join(" ");
            let st = ui.measure(&summary, &TextStyle::ui(13.5), TextOpts::wrap(w - 36.0).max_lines(4).line_height(1.4));
            let r = col.take(st.h + 196.0);
            col.gap(12.0);
            ui.card(r);
            ui.text("Saved to your library", &TextStyle::ui(12.5).semibold(), r.x + 18.0, r.y + 14.0, c.success);
            ui.g.fill_round(Rect::new(r.x + 18.0, r.y + 41.0, 4.0, 22.0), 2.0, rating_color(&m));
            ui.text_opts(&m.title, &TextStyle::ui(18.0).semibold(), TextOpts::ellipsize(w - 60.0), r.x + 30.0, r.y + 38.0, c.text);
            ui.text_opts(&format!("by {}", m.authors.join(", ")), &TextStyle::ui(13.5), TextOpts::ellipsize(w - 36.0), r.x + 18.0, r.y + 68.0, c.muted);
            let white = Color::from_u8(255, 255, 255);
            let mut tags: Vec<(String, Color, Color, bool)> = vec![(m.rating.display_name().to_string(), rating_color(&m), white, false)];
            for cat in &m.categories {
                tags.push((cat.display_name().to_string(), category_color(cat), white, false));
            }
            for f in m.fandoms.iter().take(2) {
                tags.push((f.clone(), c.border, c.text, true));
            }
            ui.g.push_clip(Rect::new(r.x + 18.0, r.y + 94.0, r.w - 36.0, 26.0));
            pills(ui, r.x + 18.0, r.y + 95.0, 10_000.0, &tags);
            ui.g.pop_clip();
            ui.g.draw_text(st, r.x + 18.0, r.y + 130.0, c.text.alpha(0.85));
            let stats = format!(
                "{} words  ·  {} chapters{}",
                thousands(m.stats.words as u64),
                m.stats.chapters_published,
                if m.is_complete { "  ·  complete" } else { "" }
            );
            ui.text(&stats, &TextStyle::ui(12.5), r.x + 18.0, r.bottom() - 46.0, c.muted);
            let bw = ui.button_width("Open in reader", Some(icons::READING));
            if ui.button_ex(id("fetch-open"), Rect::new(r.right() - 18.0 - bw, r.bottom() - 52.0, bw, 36.0), "Open in reader", Some(icons::READING), ButtonKind::Primary, true) {
                out.push(FetchAction::Open(m.id.clone()));
            }
        }

        let tips = "Paste a link to any chapter or to the work itself; the whole work is downloaded. Import file… opens EPUB books (including AO3's EPUB downloads) and AO3 HTML downloads. Works for registered users only need you to sign in under Account. Downloads are saved on this PC, so you can read them offline.";
        let t = ui.measure(tips, &TextStyle::ui(13.0), TextOpts::wrap(w - 36.0).line_height(1.45));
        let r = col.take(t.h + 54.0);
        ui.card(r);
        ui.text("Tips", &TextStyle::ui(12.5).semibold(), r.x + 18.0, r.y + 14.0, c.muted);
        ui.g.draw_text(t, r.x + 18.0, r.y + 36.0, c.muted);
    }

    fn start(&mut self, b: Busy) {
        self.busy = Some(b);
        self.error = None;
    }

    pub fn start_import(&mut self) {
        self.start(Busy::Import);
    }
}
