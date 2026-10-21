//! The look of the settings window, after macOS System Settings: it follows the system's
//! light or dark mode, uses roomy type and spacing, groups controls in rounded cards and
//! has three kinds of buttons (primary, secondary and destructive).

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText, Rounding, Stroke, TextureHandle, Vec2};

use crate::icons::{icon_rgba, IconKind};

/// Corner radius of cards and buttons.
pub(super) const RADIUS: f32 = 8.0;
/// Height of the large action buttons.
pub(super) const BUTTON_HEIGHT: f32 = 30.0;

/// Apple's system blue and red.
const ACCENT: Color32 = Color32::from_rgb(10, 132, 255);
const DANGER: Color32 = Color32::from_rgb(255, 69, 58);

/// Sets fonts, spacing, rounding and colors for both themes, following the system.
pub(super) fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    ctx.all_styles_mut(|style| {
        use egui::{FontFamily::Proportional, FontId, TextStyle};
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, Proportional)),
            (TextStyle::Body, FontId::new(14.0, Proportional)),
            (TextStyle::Button, FontId::new(14.0, Proportional)),
            (TextStyle::Small, FontId::new(12.0, Proportional)),
            (
                TextStyle::Monospace,
                FontId::new(13.0, egui::FontFamily::Monospace),
            ),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(10.0, 10.0);
        s.button_padding = Vec2::new(12.0, 5.0);
        s.interact_size.y = 26.0;
        s.combo_width = 180.0;
        s.slider_width = 220.0;
        let w = &mut style.visuals.widgets;
        for widget in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            widget.rounding = Rounding::same(6.0);
        }
        style.visuals.window_rounding = Rounding::same(RADIUS);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
        style.visuals.hyperlink_color = ACCENT;
    });
    ctx.style_mut_of(egui::Theme::Light, |style| {
        let v = &mut style.visuals;
        v.panel_fill = Color32::from_rgb(242, 242, 247);
        v.window_fill = Color32::from_rgb(242, 242, 247);
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(248, 248, 250);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(229, 229, 234);
        v.widgets.inactive.bg_fill = Color32::from_rgb(229, 229, 234);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(216, 216, 222);
    });
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let v = &mut style.visuals;
        v.panel_fill = Color32::from_rgb(28, 28, 30);
        v.window_fill = Color32::from_rgb(28, 28, 30);
        v.extreme_bg_color = Color32::from_rgb(18, 18, 20);
        v.faint_bg_color = Color32::from_rgb(36, 36, 38);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(58, 58, 60);
        v.widgets.inactive.bg_fill = Color32::from_rgb(58, 58, 60);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(72, 72, 74);
    });
}

/// The background of a card.
pub(super) fn card_fill(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(44, 44, 46)
    } else {
        Color32::WHITE
    }
}

/// The sidebar background.
pub(super) fn sidebar_fill(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(36, 36, 38)
    } else {
        Color32::from_rgb(232, 232, 237)
    }
}

/// Secondary text (descriptions, hints).
pub(super) fn subtle(ui: &egui::Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

/// A page title.
pub(super) fn page_title(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).heading().strong());
    ui.add_space(6.0);
}

/// A rounded card with an optional small caption above it.
pub(super) fn card<R>(
    ui: &mut egui::Ui,
    caption: Option<&str>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if let Some(caption) = caption {
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(caption.to_uppercase())
                    .small()
                    .color(subtle(ui)),
            )
            .truncate(),
        );
    }
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let result = egui::Frame::none()
        .fill(card_fill(ui.visuals()))
        .rounding(Rounding::same(RADIUS + 2.0))
        .stroke(Stroke::new(0.5_f32, stroke.color))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(12.0);
    result
}

/// A row inside a card: a label (and an optional description) on the left, `control` on
/// the right.
pub(super) fn row<R>(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    // The text takes at most about half the row and wraps, so the controls always fit.
    let text_width = (ui.available_width() * 0.55).clamp(140.0, 520.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.add(egui::Label::new(label).wrap());
            if let Some(d) = description {
                ui.add(egui::Label::new(RichText::new(d).small().color(subtle(ui))).wrap());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control)
            .inner
    })
    .inner
}

/// A thin line between rows of a card.
pub(super) fn divider(ui: &mut egui::Ui) {
    let color = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top();
    ui.painter()
        .hline(rect.left()..=rect.right(), y, Stroke::new(0.5_f32, color));
    ui.add_space(8.0);
}

/// Kinds of buttons.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// The main action (filled with the accent color).
    Primary,
    /// Everything else.
    Secondary,
    /// Deleting things (red text).
    Danger,
}

/// A large, rounded button, optionally with an icon.
pub(super) fn button(
    ui: &mut egui::Ui,
    kind: Kind,
    icon: Option<&TextureHandle>,
    text: impl Into<String>,
) -> egui::Response {
    let text = RichText::new(text);
    let (text, fill) = match kind {
        Kind::Primary => (text.color(Color32::WHITE).strong(), Some(ACCENT)),
        Kind::Secondary => (text, None),
        Kind::Danger => (text.color(DANGER), None),
    };
    let mut button = match icon {
        Some(tex) => egui::Button::image_and_text(
            egui::Image::new(tex).fit_to_exact_size(Vec2::splat(18.0)),
            text,
        ),
        None => egui::Button::new(text),
    }
    .min_size(Vec2::new(0.0, BUTTON_HEIGHT))
    .rounding(Rounding::same(RADIUS));
    if let Some(fill) = fill {
        button = button.fill(fill);
    }
    ui.add(button)
}

/// Textures for icons: Venecita's colored tiles and the apps' own icons, loaded once.
#[derive(Default)]
pub(super) struct Icons {
    tiles: HashMap<IconKind, TextureHandle>,
    apps: HashMap<String, Option<TextureHandle>>,
}

impl Icons {
    /// One of Venecita's colored icons.
    pub(super) fn tile(&mut self, ctx: &egui::Context, kind: IconKind) -> TextureHandle {
        self.tiles
            .entry(kind)
            .or_insert_with(|| {
                const SIZE: u32 = 48;
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [SIZE as usize, SIZE as usize],
                    &icon_rgba(kind, SIZE),
                );
                ctx.load_texture(format!("tile-{kind:?}"), image, Default::default())
            })
            .clone()
    }

    /// An app's own icon, if the system has one for `app_id`.
    pub(super) fn app(&mut self, ctx: &egui::Context, app_id: &str) -> Option<TextureHandle> {
        self.apps
            .entry(app_id.to_lowercase())
            .or_insert_with(|| {
                let icon = crate::system::app_icon(app_id)?;
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                Some(ctx.load_texture(format!("app-{app_id}"), image, Default::default()))
            })
            .clone()
    }
}

/// An app as a rounded "pill": its icon and its name. Returns the pill's response and
/// whether its "×" (shown with `removable`) was clicked.
///
/// It is a single widget of known size, so rows of pills (`horizontal_wrapped`) move it to
/// the next line when it doesn't fit, and a long name is cut with "…".
pub(super) fn app_chip(
    ui: &mut egui::Ui,
    icons: &mut Icons,
    app_id: &str,
    label: &str,
    removable: bool,
) -> (egui::Response, bool) {
    const HEIGHT: f32 = 28.0;
    const PAD: f32 = 9.0;
    const ICON: f32 = 20.0;
    const GAP: f32 = 6.0;
    const CLOSE: f32 = 16.0;

    let close_width = if removable { GAP + CLOSE } else { 0.0 };
    let max_text =
        (ui.available_width().max(140.0) - 2.0 * PAD - ICON - GAP - close_width).max(40.0);
    let text_color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_string(),
        egui::TextStyle::Body.resolve(ui.style()),
        text_color,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: max_text,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.fonts(|f| f.layout_job(job));
    let width = PAD + ICON + GAP + galley.size().x + close_width + PAD;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), egui::Sense::click());

    let mut removed = false;
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            ui.visuals().widgets.inactive.weak_bg_fill
        };
        ui.painter().rect_filled(rect, HEIGHT / 2.0, fill);

        let icon = icons
            .app(ui.ctx(), app_id)
            .unwrap_or_else(|| icons.tile(ui.ctx(), IconKind::Arrange));
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + PAD, rect.center().y - ICON / 2.0),
            Vec2::splat(ICON),
        );
        egui::Image::new(&icon).paint_at(ui, icon_rect);

        let text_pos = egui::pos2(
            icon_rect.right() + GAP,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, text_color);

        if removable {
            let close = egui::Rect::from_center_size(
                egui::pos2(rect.right() - PAD - CLOSE / 2.0, rect.center().y),
                Vec2::splat(CLOSE),
            );
            if response.hover_pos().is_some_and(|p| close.contains(p)) {
                let hover = ui.visuals().widgets.active.weak_bg_fill;
                ui.painter()
                    .circle_filled(close.center(), CLOSE / 2.0, hover);
            }
            ui.painter().text(
                close.center(),
                egui::Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(14.0),
                text_color,
            );
            removed = response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| close.contains(p));
        }
    }
    (response.on_hover_text(app_id), removed)
}

/// An on/off switch like macOS's. Returns `true` when it was flipped.
pub(super) fn switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let size = Vec2::new(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let off = ui.visuals().widgets.inactive.weak_bg_fill;
        let fill = lerp_color(off, Color32::from_rgb(52, 199, 89), t);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, fill);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let knob = egui::pos2(x, rect.center().y);
        ui.painter().circle(
            knob,
            radius - 2.5,
            Color32::WHITE,
            Stroke::new(0.5_f32, Color32::from_black_alpha(40)),
        );
    }
    response.changed()
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}
