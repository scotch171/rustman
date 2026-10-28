use eframe::egui::{self, text::LayoutJob, Color32, FontId, RichText, TextFormat};
use serde_json::Value;

struct Palette {
    key: Color32,
    string: Color32,
    number: Color32,
    literal: Color32,
    punct: Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            key: Color32::from_rgb(156, 220, 254),
            string: Color32::from_rgb(206, 145, 120),
            number: Color32::from_rgb(181, 206, 168),
            literal: Color32::from_rgb(86, 156, 214),
            punct: Color32::from_gray(170),
        }
    } else {
        Palette {
            key: Color32::from_rgb(4, 81, 165),
            string: Color32::from_rgb(163, 21, 21),
            number: Color32::from_rgb(9, 134, 88),
            literal: Color32::from_rgb(0, 0, 255),
            punct: Color32::from_gray(90),
        }
    }
}

/// Syntax-highlight already pretty-printed JSON text.
pub fn highlight(text: &str, dark: bool) -> LayoutJob {
    let p = palette(dark);
    let font = FontId::monospace(13.0);
    let mut job = LayoutJob::default();
    let mut push = |s: &str, color: Color32| {
        job.append(s, 0.0, TextFormat::simple(font.clone(), color));
    };

    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                // A string followed by ':' is an object key.
                let rest = text[i..].trim_start_matches([' ', '\t']);
                let color = if rest.starts_with(':') { p.key } else { p.string };
                push(&text[start..i], color);
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i < bytes.len()
                    && matches!(bytes[i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    i += 1;
                }
                push(&text[start..i], p.number);
            }
            b't' | b'f' | b'n' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                push(&text[start..i], p.literal);
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n')
                {
                    i += 1;
                }
                if start == i {
                    i += 1;
                }
                push(&text[start..i], p.punct);
            }
        }
    }
    job
}

pub struct TreeState {
    /// Changing the generation resets all collapsing headers.
    pub generation: u64,
    pub open_all: Option<bool>,
}

impl Default for TreeState {
    fn default() -> Self {
        Self {
            generation: 0,
            open_all: None,
        }
    }
}

pub fn tree_view(ui: &mut egui::Ui, value: &Value, state: &TreeState) {
    let dark = ui.visuals().dark_mode;
    let p = palette(dark);
    node(ui, None, value, "$", 0, state, &p);
}

fn node(
    ui: &mut egui::Ui,
    key: Option<&str>,
    value: &Value,
    path: &str,
    depth: usize,
    state: &TreeState,
    p: &Palette,
) {
    let key_text = key.map(|k| RichText::new(format!("{k}: ")).monospace().color(p.key));

    match value {
        Value::Object(map) => {
            let summary = format!("{{…}} {} ключ(ей)", map.len());
            container(ui, key_text, summary, path, depth, state, |ui| {
                for (k, v) in map {
                    node(ui, Some(k), v, &format!("{path}.{k}"), depth + 1, state, p);
                }
            });
        }
        Value::Array(items) => {
            let summary = format!("[…] {} элемент(ов)", items.len());
            container(ui, key_text, summary, path, depth, state, |ui| {
                for (i, v) in items.iter().enumerate() {
                    let idx = i.to_string();
                    node(ui, Some(&idx), v, &format!("{path}[{i}]"), depth + 1, state, p);
                }
            });
        }
        leaf => {
            let (text, color) = match leaf {
                Value::String(s) => (format!("\"{s}\""), p.string),
                Value::Number(n) => (n.to_string(), p.number),
                Value::Bool(b) => (b.to_string(), p.literal),
                _ => ("null".to_owned(), p.literal),
            };
            let resp = ui
                .horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if let Some(k) = key_text {
                        ui.label(k);
                    }
                    ui.label(RichText::new(&text).monospace().color(color))
                })
                .inner;
            resp.on_hover_text(path).context_menu(|ui| {
                if ui.button("Копировать значение").clicked() {
                    let raw = match leaf {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    ui.ctx().copy_text(raw);
                    ui.close_menu();
                }
                if ui.button("Копировать путь").clicked() {
                    ui.ctx().copy_text(path.to_owned());
                    ui.close_menu();
                }
            });
        }
    }
}

fn container(
    ui: &mut egui::Ui,
    key_text: Option<RichText>,
    summary: String,
    path: &str,
    depth: usize,
    state: &TreeState,
    body: impl FnOnce(&mut egui::Ui),
) {
    let mut header = LayoutJob::default();
    let parts = key_text
        .into_iter()
        .chain(std::iter::once(RichText::new(summary).monospace().weak()));
    for part in parts {
        part.append_to(
            &mut header,
            ui.style(),
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    }

    egui::CollapsingHeader::new(header)
        .id_salt((path, state.generation))
        .default_open(state.open_all.unwrap_or(depth < 2))
        .show(ui, body)
        .header_response
        .on_hover_text(path)
        .context_menu(|ui| {
            if ui.button("Копировать путь").clicked() {
                ui.ctx().copy_text(path.to_owned());
                ui.close_menu();
            }
        });
}
