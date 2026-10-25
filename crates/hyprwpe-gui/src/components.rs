use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, CheckButton, ColorButton, ComboBoxText, Frame,
    Image as GtkImage, Justification, Label, Orientation, Scale, SpinButton, pango,
};
use hyprwpe_core::Wallpaper;
use std::path::Path;

pub const WALLPAPER_WIDGET_WIDTH: i32 = 160;
pub const WALLPAPER_WIDGET_HEIGHT: i32 = 205;
pub const IMAGE_FRAME_WIDTH: i32 = 150;
pub const IMAGE_FRAME_HEIGHT: i32 = 100;

pub struct WallpaperWidget;

impl WallpaperWidget {
    pub fn create<F>(wallpaper: &Wallpaper, on_click: F) -> Button
    where
        F: Fn(&str) + 'static,
    {
        let main_box = GtkBox::new(Orientation::Vertical, 5);
        main_box.set_size_request(WALLPAPER_WIDGET_WIDTH, WALLPAPER_WIDGET_HEIGHT);
        main_box.set_halign(Align::Start);
        main_box.set_hexpand(false);

        let image_frame = Frame::new(None);
        image_frame.set_size_request(IMAGE_FRAME_WIDTH, IMAGE_FRAME_HEIGHT);
        image_frame.set_halign(Align::Center);
        image_frame.set_margin_top(15);

        let image = GtkImage::new();
        image.set_pixel_size(120);
        image.set_halign(Align::Center);
        image.set_valign(Align::Center);

        let preview_path = Path::new(&wallpaper.preview_path);
        if !wallpaper.preview_path.is_empty() && preview_path.is_file() {
            image.set_from_file(Some(preview_path));
        } else {
            image.set_icon_name(Some("image-missing"));
        }

        image_frame.set_child(Some(&image));
        main_box.append(&image_frame);

        let title_label = Label::new(Some(&wallpaper.title));
        title_label.set_wrap(false);
        title_label.set_justify(Justification::Center);
        title_label.set_max_width_chars(18);
        title_label.set_lines(2);
        title_label.set_ellipsize(pango::EllipsizeMode::End);
        title_label.set_halign(Align::Center);
        title_label.set_size_request(IMAGE_FRAME_WIDTH, 35);
        main_box.append(&title_label);

        let type_color = match wallpaper.wallpaper_type.as_str() {
            "video" => "#3498db",
            "scene" => "#2ecc71",
            "web" => "#e74c3c",
            _ => "#95a5a6",
        };

        let type_label = Label::new(None);
        let markup = format!(
            "<span size='x-small' color='{}'><b>{}</b></span>",
            type_color,
            wallpaper.wallpaper_type.as_str().to_uppercase()
        );
        type_label.set_markup(&markup);
        type_label.set_halign(Align::Center);
        main_box.append(&type_label);

        let button = Button::new();
        button.set_child(Some(&main_box));
        button.add_css_class("highlight");
        button.set_hexpand(false);
        button.set_vexpand(false);
        button.set_halign(Align::Start);

        let id = wallpaper.id.clone();
        button.connect_clicked(move |_| {
            on_click(&id);
        });

        button
    }
}

pub struct PropertyControls;

impl PropertyControls {
    pub fn create_audio_toggle() -> CheckButton {
        CheckButton::with_label("Enable Audio")
    }

    pub fn create_span_toggle() -> CheckButton {
        CheckButton::with_label("Stretch across all monitors")
    }

    pub fn create_speed_control() -> (GtkBox, SpinButton) {
        let box_widget = GtkBox::new(Orientation::Horizontal, 6);
        box_widget.set_valign(Align::Center);

        box_widget.append(&Label::new(Some("Speed:")));
        let spin = SpinButton::with_range(0.1, 4.0, 0.05);
        spin.set_hexpand(true);
        box_widget.append(&spin);

        (box_widget, spin)
    }

    pub fn create_scale_selector() -> (GtkBox, ComboBoxText) {
        let box_widget = GtkBox::new(Orientation::Horizontal, 6);
        box_widget.set_valign(Align::Center);

        box_widget.append(&Label::new(Some("Scale Mode:")));
        let combo = ComboBoxText::new();
        combo.append_text("Cover");
        combo.append_text("Contain");
        combo.append_text("Fill");
        combo.set_hexpand(true);
        box_widget.append(&combo);

        (box_widget, combo)
    }

    pub fn create_dynamic_bool(label_text: &str, default_value: bool) -> CheckButton {
        let check = CheckButton::with_label(label_text);
        check.set_active(default_value);
        check
    }

    pub fn create_dynamic_slider(
        label_text: &str,
        min_val: f64,
        max_val: f64,
        step: f64,
        default_value: f64,
    ) -> (GtkBox, Scale) {
        let box_widget = GtkBox::new(Orientation::Horizontal, 6);
        box_widget.set_valign(Align::Center);

        let label = Label::new(Some(label_text));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_max_width_chars(18);
        box_widget.append(&label);

        let digits = if step < 1.0 { 2 } else { 0 };
        let scale = Scale::with_range(Orientation::Horizontal, min_val, max_val, step);
        scale.set_digits(digits);
        scale.set_draw_value(true);
        scale.set_value(default_value);
        scale.set_hexpand(true);
        box_widget.append(&scale);

        (box_widget, scale)
    }

    pub fn create_dynamic_combo(
        label_text: &str,
        options: &[serde_json::Value],
        default_value: &str,
    ) -> (GtkBox, ComboBoxText) {
        let box_widget = GtkBox::new(Orientation::Horizontal, 6);
        box_widget.set_valign(Align::Center);

        let label = Label::new(Some(label_text));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_max_width_chars(18);
        box_widget.append(&label);

        let combo = ComboBoxText::new();
        let mut active_index = 0;

        for (i, opt) in options.iter().enumerate() {
            if let Some(val_str) = opt.get("value").and_then(|v| v.as_str()) {
                let label_str = opt.get("label").and_then(|v| v.as_str()).unwrap_or(val_str);
                combo.append(Some(val_str), label_str);
                if val_str == default_value {
                    active_index = i as u32;
                }
            }
        }

        combo.set_active(Some(active_index));
        combo.set_hexpand(true);
        box_widget.append(&combo);

        (box_widget, combo)
    }

    pub fn create_dynamic_color(label_text: &str, default_rgb_str: &str) -> (GtkBox, ColorButton) {
        let box_widget = GtkBox::new(Orientation::Horizontal, 6);
        box_widget.set_valign(Align::Center);

        let label = Label::new(Some(label_text));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_max_width_chars(18);
        box_widget.append(&label);

        let color_btn = ColorButton::new();
        let parts: Vec<f32> = default_rgb_str
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();

        if parts.len() >= 3 {
            let rgba = gdk::RGBA::builder()
                .red(parts[0])
                .green(parts[1])
                .blue(parts[2])
                .alpha(1.0)
                .build();
            color_btn.set_rgba(&rgba);
        }

        color_btn.set_hexpand(true);
        box_widget.append(&color_btn);

        (box_widget, color_btn)
    }
}
