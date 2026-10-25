use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static CURRENT_APPLY_GEN: AtomicU64 = AtomicU64::new(0);

use hyprwpe_core::{AppConfig, ConfigManager, WallpaperDataManager, WallpaperProperties};
use hyprwpe_runner::{
    detect_monitors, kill_legacy_python_instances, set_wallpaper, stop_all_wallpapers,
};

use crate::components::WallpaperWidget;
use crate::dialogs::OffsetDialog;
use crate::ui_builder::{build_ui, UIWidgets};

pub struct AppState {
    pub config_manager: ConfigManager,
    pub config: AppConfig,
    pub data_manager: WallpaperDataManager,
    pub properties: HashMap<String, serde_json::Value>,
    pub monitors: Vec<String>,
    pub selected_wallpaper_id: Option<String>,
    pub current_monitor: String,
    pub search_term: String,
    pub type_filters: HashMap<String, bool>,
}

pub fn run_app() {
    let app = Application::builder()
        .application_id("dev.hyprwpe.gui")
        .build();

    app.connect_startup(|_| {
        load_css();
    });

    app.connect_activate(build_gtk_app);
    app.run();
}

fn load_css() {
    let css_provider = CssProvider::new();
    if Path::new("style.css").exists() {
        css_provider.load_from_path("style.css");
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }
}

fn build_gtk_app(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("HyprWpE Wallpaper Selector")
        .default_width(1200)
        .default_height(700)
        .decorated(false)
        .opacity(0.95)
        .build();

    let config_manager = ConfigManager::new().expect("Failed to initialize ConfigManager");

    let config = config_manager.load_config().unwrap_or_default();
    let properties = config_manager.load_properties().unwrap_or_default();
    let monitors = detect_monitors();

    let mut data_manager = WallpaperDataManager::new(&config.wallpaper_dir);
    data_manager.load_wallpaper_data();

    let mut initial_type_filters = HashMap::new();
    initial_type_filters.insert("video".to_string(), true);
    initial_type_filters.insert("scene".to_string(), true);
    initial_type_filters.insert("web".to_string(), true);

    let state = Rc::new(RefCell::new(AppState {
        config_manager,
        config,
        data_manager,
        properties,
        monitors: monitors.clone(),
        selected_wallpaper_id: None,
        current_monitor: "All Monitors".to_string(),
        search_term: String::new(),
        type_filters: initial_type_filters,
    }));

    let ui = Rc::new(build_ui(&window, &monitors));
    populate_flowbox(&ui, &state);

    // Wire Save Setup button
    let state_clone = Rc::clone(&state);
    ui.save_button.connect_clicked(move |_| {
        let st = state_clone.borrow();
        let _ = st.config_manager.save_config(&st.config);
        let _ = st.config_manager.save_properties(&st.properties);
        println!("Saved setup and properties to disk.");
    });

    // Wire Configure Offset button
    let window_clone = window.clone();
    let state_clone = Rc::clone(&state);
    ui.offset_button.connect_clicked(move |_| {
        let st = state_clone.borrow();
        let margins = st.config_manager.get_panel_margins(&st.properties);
        let dialog = Rc::new(OffsetDialog::new(&window_clone, &margins));

        let dialog_clone = Rc::clone(&dialog);
        let state_inner = Rc::clone(&state_clone);
        dialog.dialog().connect_response(move |d, res| {
            if res == gtk4::ResponseType::Ok {
                let new_margins = dialog_clone.get_margins();
                let mut st = state_inner.borrow_mut();
                if let Ok(v) = serde_json::to_value(new_margins) {
                    st.properties.insert("panel_margins".to_string(), v);
                    let _ = st.config_manager.save_properties(&st.properties);
                }
            }
            d.destroy();
        });

        dialog.dialog().present();
    });

    // Wire Refresh button
    let ui_clone = Rc::clone(&ui);
    let state_clone = Rc::clone(&state);
    ui.refresh_button.connect_clicked(move |_| {
        let mut st = state_clone.borrow_mut();
        st.data_manager.load_wallpaper_data();
        populate_flowbox(&ui_clone, &state_clone);
    });

    // Wire Stop All button
    ui.stop_button.connect_clicked(|_| {
        println!("Stopping all wallpapers and terminating legacy processes...");
        stop_all_wallpapers();
    });

    // Wire Search entry
    let ui_clone = Rc::clone(&ui);
    let state_clone = Rc::clone(&state);
    ui.search_entry.connect_search_changed(move |entry| {
        let search = entry.text().to_string();
        state_clone.borrow_mut().search_term = search;
        populate_flowbox(&ui_clone, &state_clone);
    });

    // Wire Type filters
    let filter_pairs = [
        (&ui.video_filter, "video"),
        (&ui.scene_filter, "scene"),
        (&ui.web_filter, "web"),
    ];

    for (check, type_name) in filter_pairs {
        let ui_clone = Rc::clone(&ui);
        let state_clone = Rc::clone(&state);
        let t_name = type_name.to_string();
        check.connect_toggled(move |btn| {
            let active = btn.is_active();
            state_clone.borrow_mut().type_filters.insert(t_name.clone(), active);
            populate_flowbox(&ui_clone, &state_clone);
        });
    }

    // Wire Monitor Combo Box
    let state_clone = Rc::clone(&state);
    let span_check_clone = ui.span_check.clone();
    ui.monitor_combo.connect_changed(move |combo| {
        if let Some(text) = combo.active_text() {
            let is_all = text == "All Monitors";
            span_check_clone.set_visible(is_all);
            state_clone.borrow_mut().current_monitor = text.to_string();
        }
    });

    // Wire Static Property Controls
    let state_clone = Rc::clone(&state);
    ui.audio_check.connect_toggled(move |btn| {
        let mut st = state_clone.borrow_mut();
        if let Some(wid) = st.selected_wallpaper_id.clone() {
            let mut props: WallpaperProperties = st.properties.get(&wid)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            props.audio = btn.is_active();
            if let Ok(val) = serde_json::to_value(&props) {
                st.properties.insert(wid, val);
                let _ = st.config_manager.save_properties(&st.properties);
            }
        }
    });

    let state_clone = Rc::clone(&state);
    ui.span_check.connect_toggled(move |btn| {
        let mut st = state_clone.borrow_mut();
        if let Some(wid) = st.selected_wallpaper_id.clone() {
            let mut props: WallpaperProperties = st.properties.get(&wid)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            props.span = btn.is_active();
            if let Ok(val) = serde_json::to_value(&props) {
                st.properties.insert(wid, val);
                let _ = st.config_manager.save_properties(&st.properties);
            }
        }
    });

    let state_clone = Rc::clone(&state);
    ui.speed_spin.connect_value_changed(move |spin| {
        let mut st = state_clone.borrow_mut();
        if let Some(wid) = st.selected_wallpaper_id.clone() {
            let mut props: WallpaperProperties = st.properties.get(&wid)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            props.speed = spin.value() as f32;
            if let Ok(val) = serde_json::to_value(&props) {
                st.properties.insert(wid, val);
                let _ = st.config_manager.save_properties(&st.properties);
            }
        }
    });

    let state_clone = Rc::clone(&state);
    ui.scale_combo.connect_changed(move |combo| {
        let mut st = state_clone.borrow_mut();
        if let Some(wid) = st.selected_wallpaper_id.clone() {
            if let Some(text) = combo.active_text() {
                let mut props: WallpaperProperties = st.properties.get(&wid)
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                props.scale = text.to_string();
                if let Ok(val) = serde_json::to_value(&props) {
                    st.properties.insert(wid, val);
                    let _ = st.config_manager.save_properties(&st.properties);
                }
            }
        }
    });

    // Wire Open Folder Button
    let state_clone = Rc::clone(&state);
    ui.open_folder_btn.connect_clicked(move |_| {
        let st = state_clone.borrow();
        if let Some(id) = &st.selected_wallpaper_id {
            let folder_path = PathBuf::from(&st.config.wallpaper_dir).join(id);
            if folder_path.exists() {
                let _ = Command::new("xdg-open").arg(folder_path).spawn();
            }
        }
    });

    // Wire Apply Changes button
    let state_clone = Rc::clone(&state);
    ui.apply_button.connect_clicked(move |_| {
        apply_current_wallpaper(&state_clone);
    });

    window.present();
}

fn update_scene_prop(st: &mut AppState, wid: &str, prop_key: &str, val: serde_json::Value) {
    let mut props: WallpaperProperties = st.properties
        .get(wid)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    props.scene_props.insert(prop_key.to_string(), val);
    if let Ok(json_v) = serde_json::to_value(&props) {
        st.properties.insert(wid.to_string(), json_v);
        let _ = st.config_manager.save_properties(&st.properties);
    }
}

fn select_wallpaper(
    id: &str,
    wp: &hyprwpe_core::Wallpaper,
    ui: &Rc<UIWidgets>,
    state: &Rc<RefCell<AppState>>,
) {
    {
        let mut st = state.borrow_mut();
        st.selected_wallpaper_id = Some(id.to_string());
    }

    ui.sidebar.set_visible(true);
    ui.prop_widgets_box.set_sensitive(true);
    ui.folder_id_label.set_text(&format!("ID: {id}"));

    let wp_dir = state.borrow().config.wallpaper_dir.clone();
    let wp_path = PathBuf::from(&wp_dir).join(id).join("preview.jpg");
    if wp_path.exists() {
        if let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_file_at_scale(&wp_path, 320, 180, true) {
            let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
            ui.sidebar_image.set_paintable(Some(&texture));
        } else {
            ui.sidebar_image.set_paintable(Option::<&gtk4::gdk::Paintable>::None);
        }
    } else {
        ui.sidebar_image.set_paintable(Option::<&gtk4::gdk::Paintable>::None);
    }

    // Load saved properties for this wallpaper
    let current_props = {
        let st = state.borrow();
        st.properties
            .get(id)
            .and_then(|v| serde_json::from_value::<WallpaperProperties>(v.clone()).ok())
            .unwrap_or_default()
    };

    // Update static controls
    ui.audio_check.set_active(current_props.audio);
    ui.span_check.set_active(current_props.span);
    let is_all_monitors = state.borrow().current_monitor == "All Monitors";
    ui.span_check.set_visible(is_all_monitors);
    ui.speed_spin.set_value(current_props.speed as f64);

    let scale_idx = match current_props.scale.to_lowercase().as_str() {
        "contain" => 1,
        "fill" => 2,
        _ => 0,
    };
    ui.scale_combo.set_active(Some(scale_idx));

    // Clear dynamic properties container
    while let Some(child) = ui.dynamic_props_box.first_child() {
        ui.dynamic_props_box.remove(&child);
    }

    // Populate dynamic controls from wp.properties
    let mut sorted_props: Vec<(&String, &serde_json::Value)> = wp.properties.iter().collect();
    sorted_props.sort_by_key(|(_, val)| {
        val.get("order").and_then(|o| o.as_i64()).unwrap_or(999)
    });

    for (prop_key, prop_data) in sorted_props {
        let prop_type = prop_data.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let prop_text = prop_data.get("text").and_then(|v| v.as_str()).unwrap_or(prop_key);

        let saved_val = current_props.scene_props.get(prop_key);

        match prop_type {
            "bool" => {
                let default_b = saved_val
                    .and_then(|v| v.as_bool())
                    .or_else(|| prop_data.get("value").and_then(|v| v.as_bool()))
                    .unwrap_or(false);
                let check = crate::components::PropertyControls::create_dynamic_bool(prop_text, default_b);
                let st_clone = Rc::clone(state);
                let p_key = prop_key.clone();
                let wid = id.to_string();
                check.connect_toggled(move |btn| {
                    let mut st = st_clone.borrow_mut();
                    update_scene_prop(&mut st, &wid, &p_key, serde_json::Value::Bool(btn.is_active()));
                });
                ui.dynamic_props_box.append(&check);
            }
            "slider" => {
                let min_val = prop_data.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max_val = prop_data.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0);
                let step = prop_data.get("step").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let default_val = saved_val
                    .and_then(|v| v.as_f64())
                    .or_else(|| prop_data.get("value").and_then(|v| v.as_f64()))
                    .unwrap_or(min_val);
                let (box_widget, scale) = crate::components::PropertyControls::create_dynamic_slider(
                    prop_text, min_val, max_val, step, default_val,
                );
                let st_clone = Rc::clone(state);
                let p_key = prop_key.clone();
                let wid = id.to_string();
                scale.connect_value_changed(move |sc| {
                    let mut st = st_clone.borrow_mut();
                    if let Some(num) = serde_json::Number::from_f64(sc.value()) {
                        update_scene_prop(&mut st, &wid, &p_key, serde_json::Value::Number(num));
                    }
                });
                ui.dynamic_props_box.append(&box_widget);
            }
            "combo" => {
                let empty_vec = vec![];
                let options = prop_data.get("options").and_then(|v| v.as_array()).unwrap_or(&empty_vec);
                let default_str = saved_val
                    .and_then(|v| v.as_str())
                    .or_else(|| prop_data.get("value").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let (box_widget, combo) = crate::components::PropertyControls::create_dynamic_combo(
                    prop_text, options, default_str,
                );
                let st_clone = Rc::clone(state);
                let p_key = prop_key.clone();
                let wid = id.to_string();
                combo.connect_changed(move |cb| {
                    if let Some(text) = cb.active_text() {
                        let mut st = st_clone.borrow_mut();
                        update_scene_prop(&mut st, &wid, &p_key, serde_json::Value::String(text.to_string()));
                    }
                });
                ui.dynamic_props_box.append(&box_widget);
            }
            "color" => {
                let default_rgb = saved_val
                    .and_then(|v| v.as_str())
                    .or_else(|| prop_data.get("value").and_then(|v| v.as_str()))
                    .unwrap_or("1.0 1.0 1.0");
                let (box_widget, color_btn) = crate::components::PropertyControls::create_dynamic_color(
                    prop_text, default_rgb,
                );
                let st_clone = Rc::clone(state);
                let p_key = prop_key.clone();
                let wid = id.to_string();
                color_btn.connect_color_set(move |btn| {
                    let rgba = btn.rgba();
                    let color_str = format!("{:.3} {:.3} {:.3}", rgba.red(), rgba.green(), rgba.blue());
                    let mut st = st_clone.borrow_mut();
                    update_scene_prop(&mut st, &wid, &p_key, serde_json::Value::String(color_str));
                });
                ui.dynamic_props_box.append(&box_widget);
            }
            _ => {}
        }
    }

    apply_current_wallpaper(state);
}

fn populate_flowbox(ui: &Rc<UIWidgets>, state: &Rc<RefCell<AppState>>) {
    let flowbox = &ui.flowbox;
    while let Some(child) = flowbox.first_child() {
        flowbox.remove(&child);
    }

    let filtered = {
        let st = state.borrow();
        st.data_manager.apply_filters(&st.search_term, &st.type_filters)
    };

    for wp in filtered {
        let ui_clone = Rc::clone(ui);
        let state_clone = Rc::clone(state);
        let wp_clone = wp.clone();

        let widget = WallpaperWidget::create(&wp, move |id| {
            select_wallpaper(id, &wp_clone, &ui_clone, &state_clone);
        });

        flowbox.append(&widget);
    }
}

fn apply_current_wallpaper(state: &Rc<RefCell<AppState>>) {
    let (id, monitor, wallpaper_dir, properties) = {
        let st = state.borrow();
        let id = match &st.selected_wallpaper_id {
            Some(i) => i.clone(),
            None => return,
        };
        let monitor = st.current_monitor.clone();
        let dir = PathBuf::from(&st.config.wallpaper_dir);
        let props: WallpaperProperties = st.properties
            .get(&id)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        (id, monitor, dir, props)
    };

    let target_monitors = if monitor == "All Monitors" {
        state.borrow().monitors.clone()
    } else {
        vec![monitor]
    };

    let gen = CURRENT_APPLY_GEN.fetch_add(1, Ordering::SeqCst) + 1;

    std::thread::spawn(move || {
        kill_legacy_python_instances();
        for m in target_monitors {
            if CURRENT_APPLY_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            let _ = set_wallpaper(&id, &m, &wallpaper_dir, &properties);
        }
    });
}
