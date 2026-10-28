use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, FontId, Key, KeyboardShortcut, Margin, Modifiers, RichText,
    Rounding, Sense, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};

use crate::code_view::{self, Doc, Search};
use crate::curl;
use crate::http::{self, Pending, Response};
use crate::json_view::{self, TreeState};
use crate::model::{BodyKind, KeyValue, Method, Request};
use crate::theme::{self, ghost_button, underline_tab};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReqTab {
    Params,
    Headers,
    Body,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RespTab {
    Body,
    Headers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyView {
    Pretty,
    Raw,
    Tree,
}

struct Tab {
    req: Request,
    pending: Option<(Pending, Instant)>,
    response: Option<Result<Response, String>>,
    /// Pretty-printed JSON and the raw body, with their fold / search state.
    pretty_doc: Option<Doc>,
    raw_doc: Option<Doc>,
    search: Search,
    req_tab: ReqTab,
    resp_tab: RespTab,
    body_view: BodyView,
    tree: TreeState,
}

impl Tab {
    fn open_search(&mut self) {
        self.resp_tab = RespTab::Body;
        // Search works on text, so leave the tree view.
        if self.body_view == BodyView::Tree {
            self.body_view = BodyView::Pretty;
        }
        self.search.open = true;
        self.search.focus = true;
    }

    fn new(req: Request) -> Self {
        Self {
            req,
            pending: None,
            response: None,
            pretty_doc: None,
            raw_doc: None,
            search: Search::default(),
            req_tab: ReqTab::Params,
            resp_tab: RespTab::Body,
            body_view: BodyView::Pretty,
            tree: TreeState::default(),
        }
    }
}

fn default_dark() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct Saved {
    requests: Vec<Request>,
    active: usize,
    #[serde(default = "default_dark")]
    dark: bool,
}

struct CurlImport {
    text: String,
    error: Option<String>,
}

pub struct App {
    tabs: Vec<Tab>,
    active: usize,
    dark: bool,
    curl_import: Option<CurlImport>,
    toast: Option<(String, Instant)>,
}

const SEND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
const NEW_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::T);
const CLOSE_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let saved: Option<Saved> = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY));
        let dark = saved.as_ref().map_or(true, |s| s.dark);
        let (tabs, active) = match saved {
            Some(s) if !s.requests.is_empty() => {
                let active = s.active.min(s.requests.len() - 1);
                (s.requests.into_iter().map(Tab::new).collect(), active)
            }
            _ => (vec![Tab::new(example_request())], 0),
        };
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, dark);
        Self {
            tabs,
            active,
            dark,
            curl_import: None,
            toast: None,
        }
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn new_tab(&mut self, req: Request) {
        self.tabs.push(Tab::new(req));
        self.active = self.tabs.len() - 1;
    }

    fn close_tab(&mut self, idx: usize) {
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new(Request::default()));
        }
        if self.active >= idx && self.active > 0 {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn send_active(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let repaint_ctx = ctx.clone();
        let rx = http::send(tab.req.clone(), move || repaint_ctx.request_repaint());
        tab.pending = Some((rx, Instant::now()));
    }

    fn poll_responses(&mut self) {
        for tab in &mut self.tabs {
            let Some((rx, _)) = &tab.pending else { continue };
            if let Ok(result) = rx.try_recv() {
                tab.pending = None;
                tab.tree = TreeState::default();
                tab.pretty_doc = None;
                tab.raw_doc = None;
                if let Ok(r) = &result {
                    tab.pretty_doc = r.pretty.clone().map(|p| Doc::new(p, true));
                    tab.raw_doc = Some(Doc::new(r.body.clone(), false));
                }
                if matches!(&result, Ok(r) if r.json.is_none()) && tab.body_view == BodyView::Tree {
                    tab.body_view = BodyView::Pretty;
                }
                tab.response = Some(result);
            }
        }
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let t = theme::of(ui);
        let mut close: Option<usize> = None;
        let mut duplicate: Option<usize> = None;
        let mut select: Option<usize> = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            egui::ScrollArea::horizontal()
                .max_width(ui.available_width() - 90.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (i, tab) in self.tabs.iter().enumerate() {
                            let (resp, close_clicked) = tab_widget(ui, i, tab, i == self.active);
                            if close_clicked || resp.middle_clicked() {
                                close = Some(i);
                            } else if resp.clicked() {
                                select = Some(i);
                            }
                            resp.on_hover_text(&tab.req.url).context_menu(|ui| {
                                if ui.button("Дублировать").clicked() {
                                    duplicate = Some(i);
                                    ui.close_menu();
                                }
                                if ui.button("Закрыть").clicked() {
                                    close = Some(i);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                });
            ui.add_space(6.0);
            if ghost_button(ui, RichText::new("+").size(18.0).color(t.weak))
                .on_hover_text("Новая вкладка (Cmd+T)")
                .clicked()
            {
                self.new_tab(Request::default());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let label = if self.dark { "Светлая" } else { "Тёмная" };
                if ghost_button(ui, RichText::new(label).size(12.0).color(t.weak))
                    .on_hover_text("Переключить тему")
                    .clicked()
                {
                    self.dark = !self.dark;
                    theme::apply(ui.ctx(), self.dark);
                }
            });
        });

        if let Some(i) = select {
            self.active = i;
        }
        if let Some(i) = duplicate {
            let req = self.tabs[i].req.clone();
            self.new_tab(req);
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
    }

    fn request_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = theme::of(ui);
        let dark = ui.visuals().dark_mode;
        let mut send = false;
        let mut copy_curl = false;
        let mut open_import = false;
        let mut pasted_curl: Option<Result<Request, String>> = None;
        let tab = &mut self.tabs[self.active];

        // Breadcrumb-like title, as in Postman.
        let title_hint = tab.req.title();
        ui.horizontal(|ui| {
            ui.label(RichText::new("HTTP").size(12.0).color(t.weak));
            ui.label(RichText::new("/").size(12.0).color(t.weak));
            ui.add(
                egui::TextEdit::singleline(&mut tab.req.name)
                    .hint_text(title_hint)
                    .frame(false)
                    .font(FontId::proportional(14.0))
                    .desired_width(400.0),
            );
        });
        ui.add_space(6.0);

        // URL bar: [METHOD | url........] [Send] [cURL ...]
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let right_width = 300.0;
            let bar_width = (ui.available_width() - right_width).max(200.0);

            egui::Frame::none()
                .fill(t.input)
                .stroke(Stroke::new(1.0_f32, t.border))
                .rounding(Rounding::same(4.0))
                .show(ui, |ui| {
                    ui.set_width(bar_width);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.scope(|ui| {
                            let w = &mut ui.visuals_mut().widgets;
                            for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                                s.bg_stroke = Stroke::NONE;
                            }
                            w.inactive.weak_bg_fill = Color32::TRANSPARENT;
                            ui.spacing_mut().interact_size.y = 34.0;
                            egui::ComboBox::from_id_salt("method")
                                .selected_text(
                                    RichText::new(tab.req.method.as_str())
                                        .strong()
                                        .size(13.0)
                                        .color(theme::method_color(tab.req.method, dark)),
                                )
                                .width(96.0)
                                .height(300.0)
                                .show_ui(ui, |ui| {
                                    for m in Method::ALL {
                                        ui.selectable_value(
                                            &mut tab.req.method,
                                            m,
                                            RichText::new(m.as_str())
                                                .strong()
                                                .color(theme::method_color(m, dark)),
                                        );
                                    }
                                });
                        });
                        let (sep, _) = ui.allocate_exact_size(Vec2::new(1.0, 34.0), Sense::hover());
                        ui.painter().rect_filled(sep, 0.0, t.border);

                        let url = ui.add(
                            egui::TextEdit::singleline(&mut tab.req.url)
                                .hint_text("Введите URL или вставьте cURL")
                                .frame(false)
                                .margin(Margin::symmetric(10.0, 9.0))
                                .font(FontId::proportional(14.0))
                                .desired_width(f32::INFINITY),
                        );
                        if url.changed() {
                            if tab.req.url.trim_start().starts_with("curl ") {
                                pasted_curl = Some(curl::parse(&tab.req.url));
                            } else {
                                tab.req.sync_params_from_url();
                            }
                        }
                        if url.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            send = true;
                        }
                    });
                });

            if tab.pending.is_some() {
                let cancel = egui::Button::new(RichText::new("Отмена").size(14.0).color(Color32::WHITE))
                    .fill(t.weak)
                    .min_size(Vec2::new(96.0, 36.0));
                if ui.add(cancel).clicked() {
                    tab.pending = None;
                }
            } else {
                let send_btn = egui::Button::new(RichText::new("Send").size(14.0).strong().color(Color32::WHITE))
                    .fill(t.send)
                    .stroke(Stroke::NONE)
                    .min_size(Vec2::new(96.0, 36.0));
                if ui.add(send_btn).on_hover_text("Cmd+Enter").clicked() {
                    send = true;
                }
            }
            if ghost_button(ui, RichText::new("Импорт cURL").color(t.weak)).clicked() {
                open_import = true;
            }
            if ghost_button(ui, RichText::new("</>").monospace().color(t.weak))
                .on_hover_text("Скопировать как cURL")
                .clicked()
            {
                copy_curl = true;
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            let n_params = tab.req.params.iter().filter(|p| p.enabled && !p.is_empty()).count();
            let n_headers = tab.req.headers.iter().filter(|p| p.enabled && !p.is_empty()).count();
            for (kind, label, count) in [
                (ReqTab::Params, "Params", Some(n_params)),
                (ReqTab::Headers, "Headers", Some(n_headers)),
                (ReqTab::Body, "Body", None),
            ] {
                if underline_tab(ui, tab.req_tab == kind, label, count).clicked() {
                    tab.req_tab = kind;
                }
            }
            if tab.req.body_kind != BodyKind::None {
                // little green dot next to Body, like Postman does
                let r = ui.min_rect();
                ui.painter().circle_filled(egui::pos2(r.right() + 4.0, r.center().y - 6.0), 3.0, t.green);
            }
        });
        theme::rule(ui);
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .id_salt("req_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match tab.req_tab {
                ReqTab::Params => {
                    ui.label(RichText::new("Query Params").size(12.0).color(t.weak));
                    if kv_table(ui, "params", &mut tab.req.params, "Key", "Value") {
                        tab.req.sync_url_from_params();
                    }
                }
                ReqTab::Headers => {
                    ui.label(RichText::new("Headers").size(12.0).color(t.weak));
                    kv_table(ui, "headers", &mut tab.req.headers, "Key", "Value");
                }
                ReqTab::Body => body_editor(ui, &mut tab.req),
            });

        if let Some(result) = pasted_curl {
            match result {
                Ok(req) => {
                    tab.req = req;
                    self.toast("cURL импортирован");
                }
                Err(e) => self.toast(e),
            }
        }
        if open_import {
            self.curl_import = Some(CurlImport {
                text: String::new(),
                error: None,
            });
        }
        if copy_curl {
            ctx.copy_text(curl::build(&self.tabs[self.active].req));
            self.toast("cURL скопирован в буфер обмена");
        }
        if send {
            self.send_active(ctx);
        }
    }

    fn response_view(&mut self, ui: &mut egui::Ui) {
        let t = theme::of(ui);
        let tab = &mut self.tabs[self.active];

        if let Some((_, started)) = &tab.pending {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.add(egui::Spinner::new().size(28.0).color(t.accent));
                    ui.label(
                        RichText::new(format!("Отправка запроса… {:.1} с", started.elapsed().as_secs_f32()))
                            .color(t.weak),
                    );
                });
            });
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            return;
        }

        let resp = match &tab.response {
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.label(RichText::new("Response").size(16.0).color(t.weak));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Введите URL и нажмите Send, чтобы получить ответ").color(t.weak));
                });
                return;
            }
            Some(Err(e)) => {
                ui.label(RichText::new("Response").strong());
                ui.add_space(8.0);
                egui::Frame::none()
                    .fill(t.red.gamma_multiply(0.12))
                    .stroke(Stroke::new(1.0_f32, t.red.gamma_multiply(0.6)))
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Не удалось выполнить запрос").strong().color(t.red));
                        ui.add_space(4.0);
                        ui.label(RichText::new(e).monospace());
                    });
                return;
            }
            Some(Ok(r)) => r,
        };

        // Header row: tabs on the left, status / time / size on the right.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            if underline_tab(ui, tab.resp_tab == RespTab::Body, "Body", None).clicked() {
                tab.resp_tab = RespTab::Body;
            }
            if underline_tab(ui, tab.resp_tab == RespTab::Headers, "Headers", Some(resp.headers.len())).clicked() {
                tab.resp_tab = RespTab::Headers;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let status = theme::status_color(resp.status, &t);
                ui.label(RichText::new(human_size(resp.size)).color(t.green));
                ui.label(RichText::new("Size:").color(t.weak));
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{} ms", resp.elapsed.as_millis())).color(t.green));
                ui.label(RichText::new("Time:").color(t.weak));
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{} {}", resp.status, resp.reason)).strong().color(status));
                ui.label(RichText::new("Status:").color(t.weak));
            });
        });
        theme::rule(ui);
        ui.add_space(6.0);

        if tab.resp_tab == RespTab::Headers {
            egui::ScrollArea::vertical()
                .id_salt("resp_headers")
                .auto_shrink([false, false])
                .show(ui, |ui| headers_table(ui, &resp.headers));
            return;
        }

        // Body toolbar: segmented Pretty / Raw / Tree + actions.
        ui.horizontal(|ui| {
            segmented(ui, &mut tab.body_view, resp.json.is_some());
            ui.add_space(8.0);
            let kind = if resp.json.is_some() { "JSON" } else { "Text" };
            ui.label(RichText::new(kind).size(12.0).color(t.accent));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, RichText::new("Копировать").color(t.weak)).clicked() {
                    let text = resp.pretty.clone().unwrap_or_else(|| resp.body.clone());
                    ui.ctx().copy_text(text);
                }
                let search_btn = ghost_button(ui, RichText::new("Поиск").color(t.weak))
                    .on_hover_text("⌘F");
                if search_btn.clicked() {
                    tab.search.open = !tab.search.open;
                    tab.search.focus = tab.search.open;
                    if tab.search.open && tab.body_view == BodyView::Tree {
                        tab.body_view = BodyView::Pretty;
                    }
                }
                let pretty_doc = match tab.body_view {
                    BodyView::Pretty => tab.pretty_doc.as_mut().filter(|d| d.has_blocks()),
                    _ => None,
                };
                if let Some(doc) = pretty_doc {
                    if ghost_button(ui, RichText::new("Свернуть всё").color(t.weak)).clicked() {
                        doc.fold_all();
                    }
                    if ghost_button(ui, RichText::new("Развернуть всё").color(t.weak)).clicked() {
                        doc.unfold_all();
                    }
                }
                if tab.body_view == BodyView::Tree {
                    if ghost_button(ui, RichText::new("Свернуть всё").color(t.weak)).clicked() {
                        tab.tree.generation += 1;
                        tab.tree.open_all = Some(false);
                    }
                    if ghost_button(ui, RichText::new("Развернуть всё").color(t.weak)).clicked() {
                        tab.tree.generation += 1;
                        tab.tree.open_all = Some(true);
                    }
                }
            });
        });
        ui.add_space(6.0);

        if tab.search.open && tab.body_view != BodyView::Tree {
            search_bar(ui, &mut tab.search);
            ui.add_space(6.0);
        }

        egui::Frame::none()
            .fill(t.input)
            .stroke(Stroke::new(1.0_f32, t.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(6.0))
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt(("resp_scroll", tab.body_view as u8))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let doc = match tab.body_view {
                            BodyView::Tree => {
                                if let Some(json) = &resp.json {
                                    json_view::tree_view(ui, json, &tab.tree);
                                }
                                return;
                            }
                            BodyView::Pretty => tab.pretty_doc.as_mut().or(tab.raw_doc.as_mut()),
                            BodyView::Raw => tab.raw_doc.as_mut(),
                        };
                        if let Some(doc) = doc {
                            code_view::show(ui, doc, &mut tab.search);
                        }
                    });
            });
    }

    fn curl_import_window(&mut self, ctx: &egui::Context) {
        let Some(import) = &mut self.curl_import else { return };
        let mut open = true;
        let mut action: Option<bool> = None; // Some(true) = new tab, Some(false) = replace current

        egui::Window::new(RichText::new("Импорт cURL").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([640.0, 320.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let t = theme::of(ui);
                ui.label(RichText::new("Вставьте команду curl (например, «Copy as cURL» из DevTools)").color(t.weak));
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut import.text)
                            .code_editor()
                            .desired_rows(10)
                            .desired_width(f32::INFINITY),
                    );
                });
                if let Some(err) = &import.error {
                    ui.colored_label(t.red, err);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let primary = egui::Button::new(RichText::new("Импорт").color(Color32::WHITE).strong())
                        .fill(t.accent)
                        .stroke(Stroke::NONE);
                    if ui.add(primary).on_hover_text("Открыть в новой вкладке").clicked() {
                        action = Some(true);
                    }
                    if ui.button("Заменить текущую вкладку").clicked() {
                        action = Some(false);
                    }
                });
            });

        if let Some(new_tab) = action {
            match curl::parse(&import.text) {
                Ok(req) => {
                    if new_tab {
                        self.new_tab(req);
                    } else {
                        self.tabs[self.active] = Tab::new(req);
                    }
                    self.curl_import = None;
                    self.toast("cURL импортирован");
                }
                Err(e) => import.error = Some(e),
            }
        } else if !open {
            self.curl_import = None;
        }
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed() > Duration::from_secs(3) {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(50, 50, 50))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::symmetric(14.0, 8.0))
                    .show(ui, |ui| ui.label(RichText::new(msg.as_str()).color(Color32::WHITE)));
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_responses();

        if ctx.input_mut(|i| i.consume_shortcut(&SEND)) && self.tabs[self.active].pending.is_none() {
            self.send_active(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&NEW_TAB)) {
            self.new_tab(Request::default());
        }
        if ctx.input_mut(|i| i.consume_shortcut(&CLOSE_TAB)) {
            self.close_tab(self.active);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&FIND)) {
            self.tabs[self.active].open_search();
        }

        let t = theme::palette(self.dark);
        egui::TopBottomPanel::top("tabs")
            .frame(egui::Frame::none().fill(t.tabbar).stroke(Stroke::new(1.0_f32, t.border)))
            .show_separator_line(false)
            .show(ctx, |ui| self.tab_bar(ui));
        egui::TopBottomPanel::top("request")
            .resizable(true)
            .default_height(300.0)
            .min_height(140.0)
            .frame(egui::Frame::none().fill(t.bg).inner_margin(Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| self.request_editor(ui, ctx));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(t.bg).inner_margin(Margin::symmetric(16.0, 10.0)))
            .show(ctx, |ui| self.response_view(ui));

        self.curl_import_window(ctx);
        self.show_toast(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let saved = Saved {
            requests: self.tabs.iter().map(|t| t.req.clone()).collect(),
            active: self.active,
            dark: self.dark,
        };
        eframe::set_value(storage, eframe::APP_KEY, &saved);
    }
}

/// A Postman-style request tab: coloured method, title, close cross, orange top line when active.
fn tab_widget(ui: &mut egui::Ui, idx: usize, tab: &Tab, active: bool) -> (egui::Response, bool) {
    let t = theme::of(ui);
    let dark = ui.visuals().dark_mode;
    let method_g = ui.painter().layout_no_wrap(
        tab.req.method.as_str().to_owned(),
        FontId::proportional(10.5),
        theme::method_color(tab.req.method, dark),
    );
    let title_g = ui.painter().layout_no_wrap(
        tab.req.title(),
        FontId::proportional(13.0),
        if active { t.text } else { t.weak },
    );
    let width = 14.0 + method_g.size().x + 6.0 + title_g.size().x + 10.0 + 16.0 + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::click());
    let id = egui::Id::new(("tab_close", idx));
    let close_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 18.0, rect.center().y),
        Vec2::splat(16.0),
    );
    let close_resp = ui.interact(close_rect, id, Sense::click());

    let painter = ui.painter();
    let fill = if active {
        t.bg
    } else if resp.hovered() || close_resp.hovered() {
        t.hover
    } else {
        t.tabbar
    };
    painter.rect_filled(rect, 0.0, fill);
    painter.vline(rect.right(), rect.y_range(), Stroke::new(1.0_f32, t.border));
    if active {
        let top = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), 2.0));
        painter.rect_filled(top, 0.0, t.accent);
    }

    let mut x = rect.left() + 14.0;
    painter.galley(egui::pos2(x, rect.center().y - method_g.size().y / 2.0), method_g.clone(), Color32::WHITE);
    x += method_g.size().x + 6.0;
    painter.galley(egui::pos2(x, rect.center().y - title_g.size().y / 2.0), title_g, Color32::WHITE);

    if tab.pending.is_some() {
        painter.circle_filled(close_rect.center(), 3.5, t.accent);
    } else if active || resp.hovered() || close_resp.hovered() {
        if close_resp.hovered() {
            painter.rect_filled(close_rect, Rounding::same(3.0), t.border);
        }
        let c = close_rect.center();
        let d = 3.5;
        let stroke = Stroke::new(1.3_f32, if close_resp.hovered() { t.text } else { t.weak });
        painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], stroke);
        painter.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], stroke);
    }

    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    (resp, close_resp.clicked())
}

/// Segmented control: Pretty | Raw | Tree.
fn segmented(ui: &mut egui::Ui, value: &mut BodyView, has_json: bool) {
    let t = theme::of(ui);
    egui::Frame::none()
        .fill(t.tabbar)
        .stroke(Stroke::new(1.0_f32, t.border))
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::same(2.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (view, label, enabled) in [
                    (BodyView::Pretty, "Pretty", true),
                    (BodyView::Raw, "Raw", true),
                    (BodyView::Tree, "Tree", has_json),
                ] {
                    let selected = *value == view;
                    let text = RichText::new(label)
                        .size(12.5)
                        .color(if selected { t.text } else { t.weak });
                    let btn = egui::Button::new(text)
                        .fill(if selected { t.hover } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(56.0, 22.0));
                    let resp = ui.add_enabled(enabled, btn);
                    if resp.clicked() {
                        *value = view;
                    }
                    if !enabled {
                        resp.on_disabled_hover_text("Ответ не является JSON");
                    }
                }
            });
        });
}

/// Editable key/value table with borders and an always-present empty row at the bottom.
/// Returns true if anything changed.
fn kv_table(ui: &mut egui::Ui, _id: &str, rows: &mut Vec<KeyValue>, key_hint: &str, value_hint: &str) -> bool {
    if rows.last().map_or(true, |r| !r.is_empty()) {
        rows.push(KeyValue::new("", ""));
    }
    let t = theme::of(ui);
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let last = rows.len() - 1;

    let left = ui.cursor().left();
    let total = ui.available_width() - 14.0;
    let check_w = 34.0;
    let del_w = 34.0;
    let col = ((total - check_w - del_w) / 2.0).floor();
    let row_h = 32.0;
    let xs = [left, left + check_w, left + check_w + col, left + check_w + 2.0 * col, left + total];
    let top = ui.cursor().top();
    let mut row_bottoms = Vec::with_capacity(rows.len() + 1);

    ui.spacing_mut().item_spacing.y = 0.0;
    // header
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(check_w);
        for (w, text) in [(col, key_hint), (col, value_hint)] {
            ui.add_sized(
                [w, row_h - 4.0],
                egui::Label::new(RichText::new(text).size(12.0).strong().color(t.weak)),
            );
        }
    });
    row_bottoms.push(ui.cursor().top());

    for (i, row) in rows.iter_mut().enumerate() {
        let row_top = ui.cursor().top();
        let row_rect = egui::Rect::from_min_size(egui::pos2(left, row_top), Vec2::new(total, row_h));
        let hovered = ui.rect_contains_pointer(row_rect);
        if hovered {
            ui.painter().rect_filled(row_rect, 0.0, t.hover.gamma_multiply(0.5));
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                Vec2::new(check_w, row_h),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    if i != last {
                        changed |= ui.checkbox(&mut row.enabled, "").changed();
                    }
                },
            );
            let row_enabled = row.enabled;
            let text_color = t.weak;
            for (field, hint) in [(&mut row.key, key_hint), (&mut row.value, value_hint)] {
                changed |= ui
                    .add_sized(
                        [col, row_h],
                        egui::TextEdit::singleline(field)
                            .hint_text(RichText::new(hint).color(t.weak.gamma_multiply(0.6)))
                            .text_color_opt((!row_enabled).then_some(text_color))
                            .frame(false)
                            .margin(Margin::symmetric(8.0, 8.0)),
                    )
                    .changed();
            }
            if i != last && hovered {
                if ghost_button(ui, RichText::new("✕").size(11.0).color(t.weak))
                    .on_hover_text("Удалить")
                    .clicked()
                {
                    remove = Some(i);
                }
            }
        });
        // keep rows at a fixed height regardless of contents
        let used = ui.cursor().top() - row_top;
        if used < row_h {
            ui.add_space(row_h - used);
        }
        row_bottoms.push(ui.cursor().top());
    }

    let painter = ui.painter();
    let stroke = Stroke::new(1.0_f32, t.border);
    let bottom = *row_bottoms.last().unwrap();
    painter.rect_stroke(
        egui::Rect::from_min_max(egui::pos2(xs[0], top), egui::pos2(xs[4], bottom)),
        Rounding::same(4.0),
        stroke,
    );
    for y in &row_bottoms[..row_bottoms.len() - 1] {
        painter.hline(xs[0]..=xs[4], *y, stroke);
    }
    for x in &xs[1..4] {
        painter.vline(*x, top..=bottom, stroke);
    }

    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    changed
}

fn headers_table(ui: &mut egui::Ui, headers: &[(String, String)]) {
    let t = theme::of(ui);
    egui::Grid::new("resp_headers")
        .num_columns(2)
        .striped(true)
        .spacing([24.0, 8.0])
        .min_col_width(160.0)
        .show(ui, |ui| {
            ui.label(RichText::new("Key").size(12.0).strong().color(t.weak));
            ui.label(RichText::new("Value").size(12.0).strong().color(t.weak));
            ui.end_row();
            for (k, v) in headers {
                ui.label(RichText::new(k).strong());
                ui.add(egui::Label::new(RichText::new(v).monospace()).wrap());
                ui.end_row();
            }
        });
}

fn body_editor(ui: &mut egui::Ui, req: &mut Request) {
    let t = theme::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for kind in [BodyKind::None, BodyKind::FormUrlEncoded, BodyKind::Json, BodyKind::Text] {
            ui.radio_value(&mut req.body_kind, kind, kind.label());
        }
        if req.body_kind == BodyKind::Json {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, RichText::new("Beautify").color(t.send)).clicked() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&req.body) {
                        req.body = serde_json::to_string_pretty(&v).unwrap_or_default();
                    }
                }
            });
        }
    });
    ui.add_space(8.0);

    let editor_frame = egui::Frame::none()
        .fill(t.input)
        .stroke(Stroke::new(1.0_f32, t.border))
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::same(6.0));

    match req.body_kind {
        BodyKind::None => {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("У этого запроса нет тела").color(t.weak));
            });
        }
        BodyKind::Json => {
            let dark = ui.visuals().dark_mode;
            editor_frame.show(ui, |ui| {
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    ui.fonts(|f| f.layout_job(json_view::highlight(text, dark)))
                };
                ui.add(
                    egui::TextEdit::multiline(&mut req.body)
                        .frame(false)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter),
                );
            });
            if !req.body.trim().is_empty() {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&req.body) {
                    ui.colored_label(Color32::from_rgb(230, 140, 40), format!("Невалидный JSON: {e}"));
                }
            }
        }
        BodyKind::Text => {
            editor_frame.show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut req.body)
                        .code_editor()
                        .frame(false)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
            });
        }
        BodyKind::FormUrlEncoded => {
            kv_table(ui, "form", &mut req.form, "Key", "Value");
        }
    }
}

/// Search field with match counter, previous / next, case toggle and close.
fn search_bar(ui: &mut egui::Ui, search: &mut Search) {
    let t = theme::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let edit = egui::TextEdit::singleline(&mut search.query)
            .hint_text("Поиск в ответе")
            .desired_width(260.0)
            .margin(Margin::symmetric(8.0, 5.0));
        let resp = ui.add(edit);
        if search.focus {
            resp.request_focus();
            search.focus = false;
        }
        if resp.changed() {
            search.current = 0;
            search.reveal = true;
        }
        let (enter, shift, escape) = ui.input(|i| {
            (i.key_pressed(Key::Enter), i.modifiers.shift, i.key_pressed(Key::Escape))
        });
        if resp.lost_focus() && enter {
            search.step(!shift);
            resp.request_focus();
        }
        if (resp.has_focus() || resp.lost_focus()) && escape {
            search.open = false;
        }

        let counter = match (search.query.is_empty(), search.total) {
            (true, _) => String::new(),
            (false, 0) => "Нет совпадений".to_owned(),
            (false, n) => format!("{} из {n}", search.current + 1),
        };
        let color = if !search.query.is_empty() && search.total == 0 { t.red } else { t.weak };
        ui.add_sized([110.0, 20.0], egui::Label::new(RichText::new(counter).color(color)));

        if ghost_button(ui, RichText::new("↑").color(t.text)).on_hover_text("Предыдущее (Shift+Enter)").clicked() {
            search.step(false);
        }
        if ghost_button(ui, RichText::new("↓").color(t.text)).on_hover_text("Следующее (Enter)").clicked() {
            search.step(true);
        }
        let case_color = if search.case_sensitive { t.accent } else { t.weak };
        if ghost_button(ui, RichText::new("Aa").color(case_color))
            .on_hover_text("Учитывать регистр")
            .clicked()
        {
            search.case_sensitive = !search.case_sensitive;
            search.current = 0;
            search.reveal = true;
        }
        if ghost_button(ui, RichText::new("×").size(16.0).color(t.weak)).on_hover_text("Закрыть (Esc)").clicked() {
            search.open = false;
        }
    });
}

fn human_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.2} KB", b as f64 / 1024.0),
        b => format!("{:.2} MB", b as f64 / 1024.0 / 1024.0),
    }
}

fn example_request() -> Request {
    let mut req = Request {
        url: "https://jsonplaceholder.typicode.com/posts?userId=1&_limit=5".into(),
        headers: vec![KeyValue::new("Accept", "application/json")],
        ..Default::default()
    };
    req.sync_params_from_url();
    req
}
