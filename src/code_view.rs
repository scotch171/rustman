//! Read-only code view for the response body: line numbers, Postman-style
//! folding of `{ }` / `[ ]` blocks and search with highlighted matches.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{
    self, epaint::text::cursor::CCursor, text::LayoutJob, text::LayoutSection, Color32, FontId,
    Galley, Margin, Sense, Shape, Stroke, Vec2,
};

use crate::json_view;
use crate::theme;

const FONT_SIZE: f32 = 13.0;

/// Search state shared by the Pretty and Raw views of one tab.
#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    pub case_sensitive: bool,
    /// Index of the current match.
    pub current: usize,
    /// Number of matches in the last shown document.
    pub total: usize,
    /// Scroll to (and unfold) the current match on the next frame.
    pub reveal: bool,
    pub focus: bool,
}

impl Search {
    pub fn step(&mut self, forward: bool) {
        if self.total == 0 {
            return;
        }
        self.current = if forward {
            (self.current + 1) % self.total
        } else {
            (self.current + self.total - 1) % self.total
        };
        self.reveal = true;
    }
}

pub struct Doc {
    text: String,
    json: bool,
    line_starts: Vec<usize>,
    /// Foldable blocks: opening line -> closing line.
    blocks: BTreeMap<usize, usize>,
    folded: BTreeSet<usize>,
    fold_version: u64,
    matches: Vec<Range<usize>>,
    match_key: Option<(String, bool)>,
    cache: Option<Cache>,
}

struct Cache {
    key: (bool, u64, String, bool, usize),
    visible: String,
    rows: Vec<RowInfo>,
    galley: Arc<Galley>,
    /// Visible byte range of the current match, if it is not hidden by a fold.
    current: Option<Range<usize>>,
}

struct RowInfo {
    line: usize,
    /// Byte offset of the row in the visible text.
    start: usize,
}

impl Doc {
    pub fn new(text: String, json: bool) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));

        // Pretty-printed JSON never has raw newlines inside strings, so a
        // line that ends with an opening bracket starts a block.
        let mut blocks = BTreeMap::new();
        if json {
            let mut stack = Vec::new();
            for (i, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.starts_with(['}', ']']) {
                    if let Some(open) = stack.pop() {
                        blocks.insert(open, i);
                    }
                }
                if line.ends_with(['{', '[']) {
                    stack.push(i);
                }
            }
        }

        Self {
            text,
            json,
            line_starts,
            blocks,
            folded: BTreeSet::new(),
            fold_version: 0,
            matches: Vec::new(),
            match_key: None,
            cache: None,
        }
    }

    pub fn has_blocks(&self) -> bool {
        !self.blocks.is_empty()
    }

    fn line(&self, i: usize) -> &str {
        let start = self.line_starts[i];
        let end = self
            .line_starts
            .get(i + 1)
            .map_or(self.text.len(), |next| next - 1);
        self.text[start..end].trim_end_matches('\r')
    }

    fn line_of(&self, byte: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= byte) - 1
    }

    fn toggle(&mut self, line: usize) {
        if !self.folded.remove(&line) {
            self.folded.insert(line);
        }
        self.fold_version += 1;
    }

    /// Folds every nested block, leaving the root open.
    pub fn fold_all(&mut self) {
        self.folded = self.blocks.keys().copied().filter(|&l| l > 0).collect();
        self.fold_version += 1;
    }

    pub fn unfold_all(&mut self) {
        self.folded.clear();
        self.fold_version += 1;
    }

    /// Opens every folded block that hides `line`.
    fn unfold_around(&mut self, line: usize) {
        let hiding: Vec<usize> = self
            .folded
            .range(..line)
            .copied()
            .filter(|open| self.blocks.get(open).is_some_and(|&close| close >= line))
            .collect();
        if !hiding.is_empty() {
            for open in hiding {
                self.folded.remove(&open);
            }
            self.fold_version += 1;
        }
    }

    fn update_matches(&mut self, query: &str, case_sensitive: bool) {
        if self
            .match_key
            .as_ref()
            .is_some_and(|(q, c)| q == query && *c == case_sensitive)
        {
            return;
        }
        self.match_key = Some((query.to_owned(), case_sensitive));
        self.matches = find_all(&self.text, query, case_sensitive);
    }

    fn build_cache(&mut self, ui: &egui::Ui, dark: bool, search: &Search) {
        let key = (
            dark,
            self.fold_version,
            search.query.clone(),
            search.case_sensitive,
            search.current,
        );
        if self.cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }

        let t = theme::palette(dark);
        let mut visible = String::with_capacity(self.text.len());
        let mut rows = Vec::new();
        let mut placeholders = Vec::new();
        let total = self.line_starts.len();
        let mut i = 0;
        while i < total {
            if i > 0 {
                visible.push('\n');
            }
            rows.push(RowInfo { line: i, start: visible.len() });
            visible.push_str(self.line(i));
            match self.blocks.get(&i) {
                Some(&close) if self.folded.contains(&i) => {
                    let start = visible.len();
                    visible.push('…');
                    placeholders.push(start..visible.len());
                    visible.push_str(self.line(close).trim_start());
                    i = close + 1;
                }
                _ => i += 1,
            }
        }

        // Map matches from the full text to the visible text; matches inside
        // folded blocks are simply not shown.
        let mut marks: Vec<(Range<usize>, Color32)> = placeholders
            .into_iter()
            .map(|r| (r, t.border))
            .collect();
        let mut current = None;
        for (n, m) in self.matches.iter().enumerate() {
            let line = self.line_of(m.start);
            let Ok(row) = rows.binary_search_by_key(&line, |r| r.line) else {
                continue;
            };
            let start = rows[row].start + (m.start - self.line_starts[line]);
            let range = start..start + m.len();
            let color = if n == search.current {
                current = Some(range.clone());
                t.accent.gamma_multiply(0.75)
            } else {
                Color32::from_rgba_unmultiplied(255, 196, 0, if dark { 70 } else { 110 })
            };
            marks.push((range, color));
        }
        marks.sort_by_key(|(r, _)| r.start);

        let mut job = if self.json {
            json_view::highlight(&visible, dark)
        } else {
            LayoutJob::simple(
                visible.clone(),
                FontId::monospace(FONT_SIZE),
                t.text,
                f32::INFINITY,
            )
        };
        apply_marks(&mut job, &marks);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts(|f| f.layout_job(job));
        self.cache = Some(Cache {
            key,
            visible,
            rows,
            galley,
            current,
        });
    }
}

/// All non-overlapping occurrences of `query` as byte ranges.
fn find_all(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    if !case_sensitive {
        let hay = text.to_lowercase();
        let needle = query.to_lowercase();
        // Lowercasing keeps byte offsets for virtually all text (Latin,
        // Cyrillic, …); for the rare exceptions fall back to an exact search.
        if hay.len() == text.len() {
            return hay
                .match_indices(&needle)
                .map(|(i, s)| i..i + s.len())
                .collect();
        }
    }
    text.match_indices(query)
        .map(|(i, s)| i..i + s.len())
        .collect()
}

/// Splits the job's sections so that every mark gets its background colour.
/// `marks` must be sorted and non-overlapping.
fn apply_marks(job: &mut LayoutJob, marks: &[(Range<usize>, Color32)]) {
    if marks.is_empty() {
        return;
    }
    let mut out = Vec::with_capacity(job.sections.len() + marks.len() * 2);
    let mut mi = 0;
    for section in job.sections.drain(..) {
        let (mut pos, end) = (section.byte_range.start, section.byte_range.end);
        while pos < end {
            while mi < marks.len() && marks[mi].0.end <= pos {
                mi += 1;
            }
            let (next, background) = match marks.get(mi) {
                Some((r, color)) if r.start <= pos => (r.end.min(end), Some(*color)),
                Some((r, _)) => (r.start.min(end), None),
                None => (end, None),
            };
            let mut format = section.format.clone();
            if let Some(bg) = background {
                format.background = bg;
            }
            out.push(LayoutSection {
                leading_space: if pos == section.byte_range.start {
                    section.leading_space
                } else {
                    0.0
                },
                byte_range: pos..next,
                format,
            });
            pos = next;
        }
    }
    job.sections = out;
}

pub fn show(ui: &mut egui::Ui, doc: &mut Doc, search: &mut Search) {
    let dark = ui.visuals().dark_mode;
    let t = theme::of(ui);

    let query = if search.open { search.query.as_str() } else { "" };
    doc.update_matches(query, search.case_sensitive);
    search.total = doc.matches.len();
    if search.current >= search.total {
        search.current = 0;
    }
    if search.reveal {
        if let Some(m) = doc.matches.get(search.current) {
            let line = doc.line_of(m.start);
            doc.unfold_around(line);
        }
    }
    let effective = Search {
        query: query.to_owned(),
        case_sensitive: search.case_sensitive,
        current: search.current,
        ..Default::default()
    };
    doc.build_cache(ui, dark, &effective);

    let cache = doc.cache.as_ref().unwrap();
    let galley = cache.galley.clone();

    let digits = doc.line_starts.len().to_string().len() as f32;
    let char_w = ui.fonts(|f| f.glyph_width(&FontId::monospace(FONT_SIZE), '0'));
    let numbers_w = digits * char_w + 12.0;
    let fold_w = if doc.has_blocks() { 16.0 } else { 0.0 };
    let gutter_w = numbers_w + fold_w + 6.0;

    let mut toggled = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (gutter, _) =
            ui.allocate_exact_size(Vec2::new(gutter_w, galley.size().y), Sense::hover());

        let mut layouter = |_ui: &egui::Ui, _s: &str, _wrap: f32| galley.clone();
        let mut text = cache.visible.as_str();
        let output = egui::TextEdit::multiline(&mut text)
            .frame(false)
            .margin(Margin::ZERO)
            .desired_width(f32::INFINITY)
            .layouter(&mut layouter)
            .show(ui);

        let origin = output.galley_pos;
        let clip = ui.clip_rect();
        let painter = ui.painter();
        let font = FontId::monospace(FONT_SIZE);
        for (row, info) in galley.rows.iter().zip(&cache.rows) {
            let top = origin.y + row.rect.top();
            let bottom = origin.y + row.rect.bottom();
            if bottom < clip.top() || top > clip.bottom() {
                continue;
            }
            let y = (top + bottom) / 2.0;
            painter.text(
                egui::pos2(gutter.left() + numbers_w - 6.0, y),
                egui::Align2::RIGHT_CENTER,
                (info.line + 1).to_string(),
                font.clone(),
                t.weak,
            );
            if !doc.blocks.contains_key(&info.line) {
                continue;
            }
            let rect = egui::Rect::from_min_max(
                egui::pos2(gutter.left() + numbers_w, top),
                egui::pos2(gutter.left() + numbers_w + fold_w, bottom),
            );
            let resp = ui
                .interact(rect, ui.id().with(("fold", info.line)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let folded = doc.folded.contains(&info.line);
            let color = if resp.hovered() { t.text } else { t.weak };
            let c = rect.center();
            let points = if folded {
                vec![c + Vec2::new(-2.5, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.5, 4.0)]
            } else {
                vec![c + Vec2::new(-4.0, -2.5), c + Vec2::new(4.0, -2.5), c + Vec2::new(0.0, 3.0)]
            };
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
            if resp.clicked() {
                toggled = Some(info.line);
            }
        }

        if search.reveal {
            if let Some(range) = &cache.current {
                let start = cache.visible[..range.start].chars().count();
                let end = start + cache.visible[range.clone()].chars().count();
                let a = galley.pos_from_ccursor(CCursor::new(start));
                let b = galley.pos_from_ccursor(CCursor::new(end));
                let mut rect = a.union(b).translate(origin.to_vec2()).expand(24.0);
                // Keep the gutter in view unless the match is beyond the visible width.
                if rect.right() - gutter.left() < clip.width() {
                    rect.set_left(gutter.left());
                }
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
            search.reveal = false;
        }
    });

    if let Some(line) = toggled {
        doc.toggle(line);
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {\n    \"c\": \"Тест\"\n  }\n}";

    #[test]
    fn finds_blocks() {
        let doc = Doc::new(JSON.to_owned(), true);
        let blocks: Vec<_> = doc.blocks.iter().map(|(&o, &c)| (o, c)).collect();
        assert_eq!(blocks, vec![(0, 8), (1, 4), (5, 7)]);
    }

    #[test]
    fn search_is_case_insensitive_and_unfolds() {
        assert_eq!(find_all(JSON, "тест", false).len(), 1);
        assert_eq!(find_all(JSON, "тест", true).len(), 0);

        let mut doc = Doc::new(JSON.to_owned(), true);
        doc.fold_all();
        assert_eq!(doc.folded, BTreeSet::from([1, 5]));
        doc.unfold_around(6);
        assert_eq!(doc.folded, BTreeSet::from([1]));
    }
}
