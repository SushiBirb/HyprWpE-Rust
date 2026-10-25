use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, CheckButton, ComboBoxText, FlowBox,
    Label, Orientation, Picture, PolicyType, ScrolledWindow, SearchEntry,
    SelectionMode, SpinButton,
};

use crate::components::PropertyControls;

pub struct UIWidgets {
    pub save_button: Button,
    pub offset_button: Button,
    pub refresh_button: Button,
    pub stop_button: Button,
    pub search_entry: SearchEntry,
    pub monitor_combo: ComboBoxText,
    pub video_filter: CheckButton,
    pub scene_filter: CheckButton,
    pub web_filter: CheckButton,
    pub flowbox: FlowBox,
    pub sidebar: GtkBox,
    pub sidebar_image: Picture,
    pub folder_id_label: Label,
    pub open_folder_btn: Button,
    pub prop_widgets_box: GtkBox,
    pub audio_check: CheckButton,
    pub span_check: CheckButton,
    pub speed_spin: SpinButton,
    pub scale_combo: ComboBoxText,
    pub apply_button: Button,
    pub dynamic_props_box: GtkBox,
}

pub fn build_ui(window: &ApplicationWindow, monitors: &[String]) -> UIWidgets {
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    window.set_child(Some(&main_box));

    // Header bar
    let titlebar = GtkBox::new(Orientation::Horizontal, 6);
    titlebar.set_margin_start(6);
    titlebar.set_margin_end(6);
    titlebar.set_margin_top(6);
    titlebar.set_margin_bottom(6);
    main_box.append(&titlebar);

    let save_button = Button::with_label("Save Setup");
    titlebar.append(&save_button);

    let offset_button = Button::with_label("Configure Offset");
    titlebar.append(&offset_button);

    let refresh_button = Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Refresh Wallpapers"));
    titlebar.append(&refresh_button);

    let stop_button = Button::with_label("Stop All");
    stop_button.set_tooltip_text(Some("Stop all active wallpapers and kill legacy processes"));
    titlebar.append(&stop_button);

    let search_entry = SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search wallpapers..."));
    titlebar.append(&search_entry);

    let monitor_combo = ComboBoxText::new();
    monitor_combo.append_text("All Monitors");
    for m in monitors {
        monitor_combo.append_text(m);
    }
    monitor_combo.set_active(Some(0));
    titlebar.append(&monitor_combo);

    // Main content area
    let content_box = GtkBox::new(Orientation::Horizontal, 0);
    content_box.set_vexpand(true);
    main_box.append(&content_box);

    // Type filter sidebar
    let filter_sidebar = GtkBox::new(Orientation::Vertical, 10);
    filter_sidebar.set_margin_top(10);
    filter_sidebar.set_margin_bottom(10);
    filter_sidebar.set_margin_start(10);
    filter_sidebar.set_margin_end(10);
    filter_sidebar.set_width_request(180);
    content_box.append(&filter_sidebar);

    let filter_header = Label::builder()
        .label("<big><b>Filter by Type</b></big>")
        .use_markup(true)
        .halign(Align::Start)
        .build();
    filter_sidebar.append(&filter_header);

    let video_filter = CheckButton::with_label("Video");
    video_filter.set_active(true);
    filter_sidebar.append(&video_filter);

    let scene_filter = CheckButton::with_label("Scene");
    scene_filter.set_active(true);
    filter_sidebar.append(&scene_filter);

    let web_filter = CheckButton::with_label("Web");
    web_filter.set_active(true);
    filter_sidebar.append(&web_filter);

    // Wallpaper Grid FlowBox inside ScrolledWindow
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();

    let flowbox = FlowBox::new();
    flowbox.add_css_class("highlight");
    flowbox.set_orientation(Orientation::Horizontal);
    flowbox.set_selection_mode(SelectionMode::None);
    flowbox.set_homogeneous(true);
    flowbox.set_halign(Align::Start);
    flowbox.set_hexpand(true);
    flowbox.set_valign(Align::Start);
    flowbox.set_column_spacing(10);
    flowbox.set_row_spacing(10);
    flowbox.set_min_children_per_line(1);
    flowbox.set_max_children_per_line(30);
    flowbox.set_margin_top(15);
    flowbox.set_margin_bottom(15);
    flowbox.set_margin_start(15);
    flowbox.set_margin_end(15);

    scrolled_window.set_child(Some(&flowbox));
    content_box.append(&scrolled_window);

    // Properties sidebar
    let sidebar = GtkBox::new(Orientation::Vertical, 10);
    sidebar.set_margin_top(10);
    sidebar.set_margin_bottom(10);
    sidebar.set_margin_start(10);
    sidebar.set_margin_end(10);
    sidebar.set_size_request(340, -1);
    sidebar.set_hexpand(false);

    let sidebar_header = GtkBox::new(Orientation::Horizontal, 0);
    let sidebar_title = Label::builder()
        .label("<big><b>Properties</b></big>")
        .use_markup(true)
        .halign(Align::Start)
        .hexpand(true)
        .build();
    sidebar_header.append(&sidebar_title);

    let close_button = Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("flat");
    let sidebar_clone = sidebar.clone();
    close_button.connect_clicked(move |_| {
        sidebar_clone.set_visible(false);
    });
    sidebar_header.append(&close_button);
    sidebar.append(&sidebar_header);

    let sidebar_image = Picture::new();
    sidebar_image.set_can_shrink(true);
    sidebar_image.set_content_fit(gtk4::ContentFit::Contain);
    sidebar_image.set_size_request(320, 180);
    sidebar_image.set_halign(Align::Center);
    sidebar_image.set_hexpand(false);
    sidebar_image.set_vexpand(false);
    sidebar_image.set_margin_bottom(10);
    sidebar.append(&sidebar_image);

    let folder_info_box = GtkBox::new(Orientation::Horizontal, 6);
    let folder_id_label = Label::builder()
        .label("")
        .selectable(true)
        .hexpand(true)
        .halign(Align::Start)
        .build();
    folder_info_box.append(&folder_id_label);

    let open_folder_btn = Button::from_icon_name("folder-open-symbolic");
    open_folder_btn.set_tooltip_text(Some("Open Wallpaper Folder"));
    folder_info_box.append(&open_folder_btn);
    sidebar.append(&folder_info_box);

    let prop_widgets_box = GtkBox::new(Orientation::Vertical, 10);
    prop_widgets_box.set_sensitive(false);
    sidebar.append(&prop_widgets_box);

    let audio_check = PropertyControls::create_audio_toggle();
    prop_widgets_box.append(&audio_check);

    let span_check = PropertyControls::create_span_toggle();
    prop_widgets_box.append(&span_check);

    let (speed_box, speed_spin) = PropertyControls::create_speed_control();
    prop_widgets_box.append(&speed_box);

    let (scale_box, scale_combo) = PropertyControls::create_scale_selector();
    prop_widgets_box.append(&scale_box);

    let apply_button = Button::with_label("Apply Changes");
    apply_button.set_margin_top(10);
    prop_widgets_box.append(&apply_button);

    let dynamic_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(150)
        .build();

    let dynamic_props_box = GtkBox::new(Orientation::Vertical, 10);
    dynamic_scroll.set_child(Some(&dynamic_props_box));
    sidebar.append(&dynamic_scroll);

    sidebar.set_visible(false);
    content_box.append(&sidebar);

    UIWidgets {
        save_button,
        offset_button,
        refresh_button,
        stop_button,
        search_entry,
        monitor_combo,
        video_filter,
        scene_filter,
        web_filter,
        flowbox,
        sidebar,
        sidebar_image,
        folder_id_label,
        open_folder_btn,
        prop_widgets_box,
        audio_check,
        span_check,
        speed_spin,
        scale_combo,
        apply_button,
        dynamic_props_box,
    }
}
