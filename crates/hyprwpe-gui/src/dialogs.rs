use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Dialog, Grid, Label, Orientation, ResponseType, SpinButton};
use hyprwpe_core::PanelMargins;

pub struct OffsetDialog {
    dialog: Dialog,
    top_spin: SpinButton,
    bottom_spin: SpinButton,
    left_spin: SpinButton,
    right_spin: SpinButton,
}

impl OffsetDialog {
    pub fn new(parent: &impl IsA<gtk4::Window>, current_margins: &PanelMargins) -> Self {
        let dialog = Dialog::builder()
            .title("Configure Panel Margins")
            .transient_for(parent)
            .modal(true)
            .build();

        let main_box = GtkBox::new(Orientation::Vertical, 10);
        main_box.set_margin_top(10);
        main_box.set_margin_bottom(10);
        main_box.set_margin_start(10);
        main_box.set_margin_end(10);
        dialog.set_child(Some(&main_box));

        let description = Label::builder()
            .label("Set margins (in pixels) to prevent Web and Scene wallpapers from drawing under panels like Waybar.")
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .build();
        main_box.append(&description);

        let grid = Grid::builder()
            .column_spacing(10)
            .row_spacing(10)
            .margin_top(15)
            .halign(Align::Center)
            .build();
        main_box.append(&grid);

        let top_spin = SpinButton::with_range(0.0, 1000.0, 1.0);
        let bottom_spin = SpinButton::with_range(0.0, 1000.0, 1.0);
        let left_spin = SpinButton::with_range(0.0, 1000.0, 1.0);
        let right_spin = SpinButton::with_range(0.0, 1000.0, 1.0);

        top_spin.set_value(current_margins.top as f64);
        bottom_spin.set_value(current_margins.bottom as f64);
        left_spin.set_value(current_margins.left as f64);
        right_spin.set_value(current_margins.right as f64);

        grid.attach(&Label::new(Some("Top:")), 0, 0, 1, 1);
        grid.attach(&top_spin, 1, 0, 1, 1);
        grid.attach(&Label::new(Some("Bottom:")), 0, 1, 1, 1);
        grid.attach(&bottom_spin, 1, 1, 1, 1);
        grid.attach(&Label::new(Some("Left:")), 0, 2, 1, 1);
        grid.attach(&left_spin, 1, 2, 1, 1);
        grid.attach(&Label::new(Some("Right:")), 0, 3, 1, 1);
        grid.attach(&right_spin, 1, 3, 1, 1);

        dialog.add_action_widget(&Button::with_label("Cancel"), ResponseType::Cancel);
        dialog.add_action_widget(&Button::with_label("Save"), ResponseType::Ok);

        Self {
            dialog,
            top_spin,
            bottom_spin,
            left_spin,
            right_spin,
        }
    }

    pub fn dialog(&self) -> &Dialog {
        &self.dialog
    }

    pub fn get_margins(&self) -> PanelMargins {
        PanelMargins {
            top: self.top_spin.value() as u32,
            bottom: self.bottom_spin.value() as u32,
            left: self.left_spin.value() as u32,
            right: self.right_spin.value() as u32,
        }
    }
}
