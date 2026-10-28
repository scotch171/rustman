use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, Rounding, Sense, Stroke,
    TextStyle, Vec2,
};

use crate::model::Method;

/// Postman-like palette. Picked from the current visuals, so no state needs threading.
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color32,
    pub tabbar: Color32,
    pub input: Color32,
    pub border: Color32,
    pub hover: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub send: Color32,
    pub green: Color32,
    pub red: Color32,
}

const DARK: Theme = Theme {
    bg: Color32::from_rgb(33, 33, 33),
    tabbar: Color32::from_rgb(26, 26, 26),
    input: Color32::from_rgb(28, 28, 28),
    border: Color32::from_rgb(58, 58, 58),
    hover: Color32::from_rgb(48, 48, 48),
    text: Color32::from_rgb(235, 235, 235),
    weak: Color32::from_rgb(150, 150, 150),
    accent: Color32::from_rgb(255, 108, 55),
    send: Color32::from_rgb(9, 123, 237),
    green: Color32::from_rgb(12, 187, 82),
    red: Color32::from_rgb(247, 79, 68),
};

const LIGHT: Theme = Theme {
    bg: Color32::from_rgb(255, 255, 255),
    tabbar: Color32::from_rgb(245, 245, 245),
    input: Color32::from_rgb(255, 255, 255),
    border: Color32::from_rgb(225, 225, 225),
    hover: Color32::from_rgb(237, 237, 237),
    text: Color32::from_rgb(33, 33, 33),
    weak: Color32::from_rgb(107, 107, 107),
    accent: Color32::from_rgb(255, 108, 55),
    send: Color32::from_rgb(9, 123, 237),
    green: Color32::from_rgb(0, 127, 49),
    red: Color32::from_rgb(200, 40, 30),
};

pub fn palette(dark: bool) -> Theme {
    if dark {
        DARK
    } else {
        LIGHT
    }
}

pub fn of(ui: &egui::Ui) -> Theme {
    palette(ui.visuals().dark_mode)
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let t = palette(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let rounding = Rounding::same(4.0);

    v.panel_fill = t.bg;
    v.window_fill = t.bg;
    v.window_stroke = Stroke::new(1.0_f32, t.border);
    v.window_rounding = Rounding::same(6.0);
    v.extreme_bg_color = t.input;
    v.faint_bg_color = t.tabbar;
    v.selection.bg_fill = t.send.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, t.text);
    v.hyperlink_color = t.send;

    v.widgets.noninteractive.bg_fill = t.bg;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, t.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, t.text);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = rounding;
        w.expansion = 0.0;
    }
    v.widgets.inactive.weak_bg_fill = t.tabbar;
    v.widgets.inactive.bg_fill = t.tabbar;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, t.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, t.weak);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.active.weak_bg_fill = t.hover;
    v.widgets.active.bg_fill = t.hover;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, t.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.open.weak_bg_fill = t.hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, t.border);

    // Pin the theme explicitly: by default egui follows the OS theme and would
    // swap our visuals for its stock ones.
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |s| {
        s.visuals = v;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.window_margin = Margin::same(14.0);
        s.text_styles = [
            (TextStyle::Heading, FontId::proportional(17.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
            (TextStyle::Small, FontId::proportional(11.0)),
        ]
        .into();
    });
}

/// Use the OS UI fonts when available (they look much closer to Postman than
/// egui's bundled ones); the bundled fonts stay as fallbacks for icons/emoji.
pub fn install_fonts(ctx: &egui::Context) {
    const SANS: &[&str] = &[
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    const MONO: &[&str] = &[
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "C:\\Windows\\Fonts\\consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ];

    let mut fonts = FontDefinitions::default();
    for (name, paths, family) in [
        ("ui-sans", SANS, FontFamily::Proportional),
        ("ui-mono", MONO, FontFamily::Monospace),
    ] {
        if let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts
                .font_data
                .insert(name.to_owned(), FontData::from_owned(bytes));
            fonts.families.entry(family).or_default().insert(0, name.to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn method_color(m: Method, dark: bool) -> Color32 {
    let (d, l) = match m {
        Method::Get => ((12, 187, 82), (0, 127, 49)),
        Method::Post => ((255, 180, 0), (173, 122, 3)),
        Method::Put => ((9, 123, 237), (9, 90, 180)),
        Method::Patch => ((180, 130, 255), (98, 58, 162)),
        Method::Delete => ((247, 79, 68), (142, 26, 16)),
        Method::Head => ((12, 187, 82), (0, 127, 49)),
        Method::Options => ((240, 100, 180), (165, 20, 100)),
    };
    let (r, g, b) = if dark { d } else { l };
    Color32::from_rgb(r, g, b)
}

pub fn status_color(status: u16, t: &Theme) -> Color32 {
    match status {
        200..=299 => t.green,
        300..=399 => t.send,
        400..=499 => Color32::from_rgb(230, 140, 40),
        _ => t.red,
    }
}

/// Text tab with an orange underline when selected (Params / Headers / Body ...).
pub fn underline_tab(ui: &mut egui::Ui, selected: bool, label: &str, count: Option<usize>) -> egui::Response {
    let t = of(ui);
    let font = FontId::proportional(13.5);
    let measure = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), t.weak);
    let count_g = count
        .filter(|n| *n > 0)
        .map(|n| ui.painter().layout_no_wrap(format!(" {n}"), FontId::proportional(11.5), t.green));
    let width = measure.size().x + count_g.as_ref().map_or(0.0, |g| g.size().x) + 4.0;

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if selected || resp.hovered() { t.text } else { t.weak };
    let label_g = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let y = rect.center().y - label_g.size().y / 2.0;
    ui.painter().galley(egui::pos2(rect.left(), y), label_g.clone(), color);
    if let Some(g) = count_g {
        let x = rect.left() + label_g.size().x;
        ui.painter().galley(egui::pos2(x, y - 2.0), g, t.green);
    }
    if selected {
        let line = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 2.0),
            egui::pos2(rect.left() + label_g.size().x, rect.bottom()),
        );
        ui.painter().rect_filled(line, 0.0, t.accent);
    }
    resp
}

/// Thin full-width horizontal rule in the border colour.
pub fn rule(ui: &mut egui::Ui) {
    let t = of(ui);
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top();
    ui.painter().hline(rect.x_range(), y, Stroke::new(1.0_f32, t.border));
    ui.add_space(1.0);
}

/// Flat button without background until hovered.
pub fn ghost_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets.inactive;
        w.weak_bg_fill = Color32::TRANSPARENT;
        w.bg_stroke = Stroke::NONE;
        ui.add(egui::Button::new(text))
    })
    .inner
}
