//! The full-window media viewer (`viewer_overlay`, Arch §9).
//!
//! Mounted at `app_overlay` so it covers the sidebar and header. Navigation
//! walks a stable id snapshot captured at open time (a live query replacement
//! cannot change the set mid-session); each load bumps a generation so a slow
//! off-thread decode only installs when still current (Arch §5). The Info/Tags
//! side panel is strictly read-only: filesystem/application metadata and
//! folder-derived chips only — no EXIF, location, hashes, or tag editing.

use libadwaita as adw;
use libadwaita::gtk::{self, gio, glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Callback invoked when the viewer closes, carrying the model position to
/// restore focus/scroll to (the origin item or its nearest surviving neighbour).
pub type OnClose = Rc<dyn Fn(u32)>;

pub struct Viewer {
    /// Root widget mounted at `app_overlay`; `.viewer` drives the open fade.
    pub overlay: gtk::Overlay,
    // Ordered stable media identities captured when the viewer opened.
    snapshot: RefCell<Vec<i64>>,
    snapshot_index: RefCell<usize>,
    origin_snapshot_index: RefCell<usize>,
    filter_model: gtk::SortListModel,
    on_close: RefCell<Option<OnClose>>,
    media_stack: gtk::Stack,
    stage: gtk::Overlay,
    image_scrolled_window: gtk::ScrolledWindow,
    picture: gtk::Picture,
    video_picture: gtk::Picture,
    media_stream: RefCell<Option<gtk::MediaStream>>,
    zoom_level: RefCell<f64>,
    // Bumped on every load so a stale off-thread decode is discarded on arrival.
    media_generation: Rc<Cell<u64>>,
    controls_visible: RefCell<bool>,
    nav_buttons: Vec<gtk::Button>,
    topbar: gtk::Box,
    filename_pill: gtk::Box,
    zoom_controls: gtk::Box,
    zoom_label: gtk::Label,
    // Current media path, for the overflow menu (open / reveal / copy path).
    current_path: RefCell<String>,
    info_revealer: gtk::Revealer,
    breadcrumb: gtk::Label,
    pill_name: gtk::Label,
    pill_position: gtk::Label,
    v_filename: gtk::Label,
    v_type: gtk::Label,
    v_added: gtk::Label,
    v_modified: gtk::Label,
    v_dimensions: gtk::Label,
    v_duration: gtk::Label,
    duration_row: gtk::Box,
    v_folder: gtk::Label,
    v_source: gtk::Label,
    tags_flow: gtk::FlowBox,
    error_label: gtk::Label,
    play_btn: gtk::Button,
    time_label: gtk::Label,
    seek_adj: gtk::Adjustment,
    loop_btn: gtk::ToggleButton,
    vol_bar: gtk::Scale,
}

impl Viewer {
    pub fn new(filter_model: gtk::SortListModel) -> Rc<Self> {
        // ── Media stage ────────────────────────────────────────────────────
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .can_shrink(true)
            .build();
        let image_scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&picture)
            .build();

        let video_picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .build();
        let video_overlay = gtk::Overlay::builder().build();
        video_overlay.set_child(Some(&video_picture));

        let (video_controls_box, play_btn, seek_bar, time_label, vol_btn, vol_bar, loop_btn) =
            build_video_controls();
        video_overlay.add_overlay(&video_controls_box);

        // Static placeholders for loading and decode failure (Visual §6).
        let loading_box = placeholder_box("image-x-generic-symbolic", "");
        let error_box = placeholder_box(
            "network-offline-symbolic",
            "This file is currently unavailable.",
        );
        let error_label = error_box
            .last_child()
            .and_downcast::<gtk::Label>()
            .expect("error placeholder has a label");

        let media_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(150)
            .hexpand(true)
            .vexpand(true)
            .build();
        media_stack.add_named(&loading_box, Some("loading"));
        media_stack.add_named(&image_scrolled_window, Some("image"));
        media_stack.add_named(&video_overlay, Some("video"));
        media_stack.add_named(&error_box, Some("error"));

        let stage = gtk::Overlay::builder()
            .css_classes(["viewer-stage"])
            .hexpand(true)
            .vexpand(true)
            .build();
        stage.set_child(Some(&media_stack));

        // Filename pill (top center) and nav arrows.
        let (filename_pill, pill_name, pill_position) = build_filename_pill();
        stage.add_overlay(&filename_pill);

        let prev_btn = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .css_classes(["osd", "viewer-nav"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .build();
        prev_btn.update_property(&[gtk::accessible::Property::Label("Previous")]);
        let next_btn = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["osd", "viewer-nav"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .build();
        next_btn.update_property(&[gtk::accessible::Property::Label("Next")]);
        stage.add_overlay(&prev_btn);
        stage.add_overlay(&next_btn);

        // Zoom controls (bottom center): fit, −, level, +, fullscreen.
        let (zoom_controls, zoom_fit_btn, zoom_out_btn, zoom_label, zoom_in_btn, zoom_fs_btn) =
            build_zoom_controls();
        stage.add_overlay(&zoom_controls);

        // ── Top bar: brand + breadcrumb + controls ─────────────────────────
        let (
            topbar,
            breadcrumb,
            panel_toggle,
            fullscreen_btn,
            menu_btn,
            close_btn,
            menu_open,
            menu_reveal,
            menu_copy,
        ) = build_topbar();

        let viewer_main = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        viewer_main.append(&topbar);
        viewer_main.append(&stage);

        // ── Read-only Info / Tags panel ────────────────────────────────────
        let (info_revealer, view_bits) = build_info_panel();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .vexpand(true)
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .build();
        content.append(&viewer_main);
        content.append(&info_revealer);

        // A GtkOverlay fills its *main* child (set_child) but gives add_overlay
        // children only their natural size. So `content` must be the main child
        // to span the full window and cover the grid/sidebar/header (Arch §9).
        // Nav/zoom/pill OSD widgets are overlaid on the inner `stage`, not here.
        let root = gtk::Overlay::builder()
            .css_classes(["viewer", "viewer-bg"])
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();
        root.set_child(Some(&content));

        let viewer = Rc::new(Self {
            overlay: root,
            snapshot: RefCell::new(Vec::new()),
            snapshot_index: RefCell::new(0),
            origin_snapshot_index: RefCell::new(0),
            filter_model,
            on_close: RefCell::new(None),
            media_stack,
            stage,
            image_scrolled_window: image_scrolled_window.clone(),
            picture,
            video_picture,
            media_stream: RefCell::new(None),
            zoom_level: RefCell::new(0.0),
            media_generation: Rc::new(Cell::new(0)),
            controls_visible: RefCell::new(true),
            nav_buttons: vec![prev_btn.clone(), next_btn.clone()],
            topbar,
            filename_pill,
            zoom_controls,
            zoom_label,
            current_path: RefCell::new(String::new()),
            info_revealer: info_revealer.clone(),
            breadcrumb,
            pill_name,
            pill_position,
            v_filename: view_bits.filename,
            v_type: view_bits.type_,
            v_added: view_bits.added,
            v_modified: view_bits.modified,
            v_dimensions: view_bits.dimensions,
            v_duration: view_bits.duration,
            duration_row: view_bits.duration_row,
            v_folder: view_bits.folder,
            v_source: view_bits.source,
            tags_flow: view_bits.tags_flow,
            error_label,
            play_btn: play_btn.clone(),
            time_label: time_label.clone(),
            seek_adj: seek_bar.adjustment(),
            loop_btn: loop_btn.clone(),
            vol_bar: vol_bar.clone(),
        });

        viewer.wire_video_controls(&play_btn, &seek_bar, &vol_bar, &vol_btn, &loop_btn);
        viewer.wire_stage_gestures(&video_overlay);
        viewer.wire_keyboard();

        // Navigation + chrome buttons.
        prev_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.prev()
        });
        next_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.next()
        });
        close_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.close()
        });
        panel_toggle.connect_clicked({
            let rev = info_revealer.clone();
            move |_| toggle_info_panel(&rev)
        });
        fullscreen_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.toggle_fullscreen()
        });
        zoom_fs_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.toggle_fullscreen()
        });
        zoom_fit_btn.connect_clicked({
            let v = viewer.clone();
            move |_| {
                *v.zoom_level.borrow_mut() = 0.0;
                v.apply_zoom();
                v.update_zoom_label();
            }
        });
        zoom_in_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.zoom_step(true)
        });
        zoom_out_btn.connect_clicked({
            let v = viewer.clone();
            move |_| v.zoom_step(false)
        });

        // Overflow menu: Open externally / Reveal in Folder / Copy Path.
        menu_open.connect_clicked({
            let v = viewer.clone();
            move |_| launch_path(&v.current_path.borrow(), false)
        });
        menu_reveal.connect_clicked({
            let v = viewer.clone();
            move |_| launch_path(&v.current_path.borrow(), true)
        });
        menu_copy.connect_clicked({
            let v = viewer.clone();
            move |_| {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&v.current_path.borrow());
                }
            }
        });
        let _ = menu_btn;

        viewer
    }

    /// Sets the callback used to restore grid focus/scroll after close.
    pub fn set_on_close(&self, cb: OnClose) {
        *self.on_close.borrow_mut() = Some(cb);
    }

    pub fn is_open(&self) -> bool {
        self.overlay.is_visible()
    }

    fn is_fullscreen(&self) -> bool {
        self.overlay
            .root()
            .and_downcast::<gtk::Window>()
            .is_some_and(|window| window.is_fullscreen())
    }

    /// Escape precedence (Product §5): fullscreen exits before the viewer
    /// closes. Returns whether the viewer consumed the Escape.
    pub fn handle_escape(&self) -> bool {
        if !self.is_open() {
            return false;
        }
        if self.is_fullscreen() {
            self.toggle_fullscreen();
        } else {
            self.close();
        }
        true
    }

    fn model_position_of(&self, media_id: i64) -> Option<u32> {
        (0..self.filter_model.n_items()).find(|&i| {
            self.filter_model
                .item(i)
                .and_downcast::<crate::ui::model::MediaItem>()
                .is_some_and(|item| item.property::<i64>("id") == media_id)
        })
    }

    fn show_unavailable(&self) {
        self.media_generation
            .set(self.media_generation.get().wrapping_add(1));
        self.error_label
            .set_text("This file is currently unavailable.");
        self.media_stack.set_visible_child_name("error");
    }

    fn load_snapshot_item(&self, index: usize) {
        let media_id = match self.snapshot.borrow().get(index) {
            Some(id) => *id,
            None => return,
        };
        match self.model_position_of(media_id) {
            Some(position) => self.load_item(position),
            None => self.show_unavailable(),
        }
    }

    pub fn open(&self, position: u32) {
        let n_items = self.filter_model.n_items();
        if position >= n_items {
            return;
        }
        // Capture the current filtered/sorted list as stable identities;
        // navigation uses only this snapshot until close (Arch §5).
        let snapshot: Vec<i64> = (0..n_items)
            .filter_map(|i| {
                self.filter_model
                    .item(i)
                    .and_downcast::<crate::ui::model::MediaItem>()
            })
            .map(|item| item.property::<i64>("id"))
            .collect();
        *self.snapshot.borrow_mut() = snapshot;
        *self.snapshot_index.borrow_mut() = position as usize;
        *self.origin_snapshot_index.borrow_mut() = position as usize;

        self.load_snapshot_item(position as usize);
        self.overlay.set_visible(true);
        let overlay = self.overlay.clone();
        glib::idle_add_local_once(move || overlay.add_css_class("open"));
    }

    pub fn close(&self) {
        if self.is_fullscreen() {
            self.toggle_fullscreen();
        }
        self.overlay.remove_css_class("open");

        if let Some(stream) = self.media_stream.borrow().as_ref() {
            stream.pause();
        }
        *self.media_stream.borrow_mut() = None;
        self.video_picture
            .set_paintable(None::<&gtk::gdk::Paintable>);

        let overlay = self.overlay.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(180), move || {
            overlay.set_visible(false);
        });

        // Resolve the opening origin against the current grid; if it vanished,
        // fall back to the nearest surviving snapshot neighbour.
        let snapshot = self.snapshot.borrow().clone();
        let origin = *self.origin_snapshot_index.borrow();
        let mut pos: Option<u32> = None;
        for delta in 0..snapshot.len() {
            for candidate in [origin.checked_sub(delta), origin.checked_add(delta)] {
                if let Some(id) = candidate.and_then(|i| snapshot.get(i))
                    && let Some(found) = self.model_position_of(*id)
                {
                    pos = Some(found);
                    break;
                }
            }
            if pos.is_some() {
                break;
            }
        }
        if let Some(pos) = pos
            && let Some(cb) = self.on_close.borrow().as_ref()
        {
            cb(pos);
        }
    }

    pub fn next(&self) {
        let snapshot = self.snapshot.borrow().clone();
        let len = snapshot.len();
        if len == 0 {
            return;
        }
        let start = *self.snapshot_index.borrow();
        for step in 1..=len {
            let idx = (start + step) % len;
            if self.model_position_of(snapshot[idx]).is_some() {
                *self.snapshot_index.borrow_mut() = idx;
                self.load_snapshot_item(idx);
                return;
            }
        }
        self.show_unavailable();
    }

    pub fn prev(&self) {
        let snapshot = self.snapshot.borrow().clone();
        let len = snapshot.len();
        if len == 0 {
            return;
        }
        let start = *self.snapshot_index.borrow();
        for step in 1..=len {
            let idx = (start + len - (step % len)) % len;
            if self.model_position_of(snapshot[idx]).is_some() {
                *self.snapshot_index.borrow_mut() = idx;
                self.load_snapshot_item(idx);
                return;
            }
        }
        self.show_unavailable();
    }

    fn load_item(&self, position: u32) {
        self.media_generation
            .set(self.media_generation.get().wrapping_add(1));

        if let Some(stream) = self.media_stream.borrow().as_ref() {
            stream.pause();
        }
        *self.media_stream.borrow_mut() = None;
        self.video_picture
            .set_paintable(None::<&gtk::gdk::Paintable>);
        self.picture.set_paintable(None::<&gtk::gdk::Paintable>);

        let Some(obj) = self.filter_model.item(position) else {
            return;
        };
        let Some(media_item) = obj.downcast_ref::<crate::ui::model::MediaItem>() else {
            return;
        };

        let path: String = media_item.property("path");
        let filename: String = media_item.property("filename");
        let is_video: bool = media_item.property("is-video");
        *self.current_path.borrow_mut() = path.clone();

        *self.zoom_level.borrow_mut() = 0.0;
        self.apply_zoom();
        self.update_zoom_label();

        // Filename pill + breadcrumb position "N / M".
        let total = self.snapshot.borrow().len();
        let index = *self.snapshot_index.borrow();
        self.pill_name.set_text(&filename);
        self.pill_position
            .set_text(&format!("{} / {}", index + 1, total));
        self.breadcrumb
            .set_text(&format!("Library  /  {total} items"));

        // ── Read-only Info panel ───────────────────────────────────────────
        self.v_filename.set_text(&filename);
        self.v_type.set_text(&media_type_label(&filename, is_video));
        self.v_added
            .set_text(&format_timestamp(media_item.property("date-added")));
        self.v_modified
            .set_text(&format_timestamp(media_item.property("modified-at")));
        self.v_folder
            .set_text(&parent_folder_name(&path).unwrap_or_default());
        self.v_source.set_text(&path);
        self.populate_tags(&media_item.property::<String>("tags"));

        if is_video {
            self.duration_row.set_visible(true);
            let dur: i64 = media_item.property("duration-secs");
            self.v_duration.set_text(&if dur > 0 {
                format_hms(dur)
            } else {
                "Unknown".into()
            });
        } else {
            self.duration_row.set_visible(false);
        }

        let is_offline: bool = media_item.property("is-offline");
        if is_offline {
            self.v_dimensions.set_text("—");
            self.show_unavailable();
            return;
        }

        if is_video {
            self.v_dimensions.set_text("—");
            self.load_video(&path);
        } else {
            self.v_dimensions.set_text("Loading…");
            self.load_image(path);
        }
    }

    fn load_video(&self, path: &str) {
        let file = gio::File::for_path(path);
        let stream = gtk::MediaFile::for_file(&file);

        let play_btn = self.play_btn.clone();
        stream.connect_playing_notify(move |s| {
            if s.is_playing() {
                play_btn.set_icon_name("media-playback-pause-symbolic");
            } else {
                play_btn.set_icon_name("media-playback-start-symbolic");
            }
        });
        let time_label = self.time_label.clone();
        let seek_adj = self.seek_adj.clone();
        stream.connect_timestamp_notify(move |s| {
            time_label.set_text(&format!(
                "{} / {}",
                format_hms(s.timestamp() / 1_000_000),
                format_hms(s.duration() / 1_000_000)
            ));
            seek_adj.set_value(s.timestamp() as f64);
        });
        let seek_adj = self.seek_adj.clone();
        stream.connect_duration_notify(move |s| seek_adj.set_upper(s.duration() as f64));
        let error_label = self.error_label.clone();
        let media_stack = self.media_stack.clone();
        stream.connect_error_notify(move |s| {
            if s.error().is_some() {
                error_label.set_text("This file could not be played.");
                media_stack.set_visible_child_name("error");
            }
        });

        stream.set_loop(self.loop_btn.is_active());
        stream.set_volume(self.vol_bar.value());
        stream.play();
        self.video_picture.set_paintable(Some(&stream));
        *self.media_stream.borrow_mut() = Some(stream.upcast());
        self.media_stack.set_visible_child_name("video");
    }

    fn load_image(&self, path: String) {
        self.media_stack.set_visible_child_name("loading");
        let dimensions = self.v_dimensions.clone();
        let picture = self.picture.clone();
        let error_label = self.error_label.clone();
        let media_stack = self.media_stack.clone();
        let generation = self.media_generation.get();
        let media_generation = self.media_generation.clone();

        glib::spawn_future_local(async move {
            // Both the read and the decode run off the GTK thread; only the
            // cheap MemoryTexture install happens on it (Arch §5).
            let decoded = tokio::task::spawn_blocking(move || {
                let rgba = image::open(&path)?.to_rgba8();
                let (w, h) = rgba.dimensions();
                Ok::<_, image::ImageError>((rgba.into_raw(), w, h))
            })
            .await;

            // Stale decode: a newer load superseded this one while it ran.
            if media_generation.get() != generation {
                return;
            }
            if let Ok(Ok((pixels, w, h))) = decoded
                && let (Ok(width), Ok(height)) = (i32::try_from(w), i32::try_from(h))
                && let Some(stride) = (w as usize).checked_mul(4)
            {
                let bytes = glib::Bytes::from_owned(pixels);
                let texture = gtk::gdk::MemoryTexture::new(
                    width,
                    height,
                    gtk::gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    stride,
                );
                dimensions.set_text(&format!("{w} × {h}"));
                picture.set_paintable(Some(&texture));
                media_stack.set_visible_child_name("image");
                return;
            }
            dimensions.set_text("Unknown");
            error_label.set_text("This image could not be displayed.");
            media_stack.set_visible_child_name("error");
        });
    }

    fn populate_tags(&self, tags: &str) {
        while let Some(child) = self.tags_flow.first_child() {
            self.tags_flow.remove(&child);
        }
        for tag in tags.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let chip = gtk::Label::builder()
                .label(tag)
                .css_classes(["tag-chip"])
                .build();
            self.tags_flow.insert(&chip, -1);
        }
    }

    pub fn toggle_fullscreen(&self) {
        if let Some(window) = self.overlay.root().and_downcast::<gtk::Window>() {
            if window.is_fullscreen() {
                window.unfullscreen();
            } else {
                window.fullscreen();
            }
        }
        let visible = !*self.controls_visible.borrow();
        *self.controls_visible.borrow_mut() = visible;
        for btn in &self.nav_buttons {
            btn.set_visible(visible);
        }
        self.topbar.set_visible(visible);
        self.filename_pill.set_visible(visible);
        self.zoom_controls.set_visible(visible);
        if visible {
            self.stage.remove_css_class("fullscreen");
        } else {
            self.stage.add_css_class("fullscreen");
        }
    }

    fn zoom_step(&self, zoom_in: bool) {
        let current = *self.zoom_level.borrow();
        let base = if current == 0.0 { 1.0 } else { current };
        let next = if zoom_in { base * 1.25 } else { base / 1.25 };
        *self.zoom_level.borrow_mut() = next.clamp(0.0, 8.0);
        if *self.zoom_level.borrow() < 0.15 {
            *self.zoom_level.borrow_mut() = 0.0;
        }
        self.apply_zoom();
        self.update_zoom_label();
    }

    fn update_zoom_label(&self) {
        let zoom = *self.zoom_level.borrow();
        let text = if zoom == 0.0 {
            "Fit".to_string()
        } else if (zoom - 1.0).abs() < 0.02 {
            "1:1".to_string()
        } else {
            format!("{}%", (zoom * 100.0).round() as i32)
        };
        self.zoom_label.set_text(&text);
    }

    fn apply_zoom(&self) {
        let zoom = *self.zoom_level.borrow();
        if zoom == 0.0 {
            self.picture.set_content_fit(gtk::ContentFit::Contain);
            self.picture.set_can_shrink(true);
            self.picture.set_size_request(-1, -1);
        } else if let Some(p) = self.picture.paintable() {
            let w = p.intrinsic_width() as f64;
            let h = p.intrinsic_height() as f64;
            if w > 0.0 && h > 0.0 {
                self.picture.set_content_fit(gtk::ContentFit::Fill);
                self.picture.set_can_shrink(false);
                self.picture
                    .set_size_request((w * zoom) as i32, (h * zoom) as i32);
            }
        }
    }

    fn wire_video_controls(
        self: &Rc<Self>,
        play_btn: &gtk::Button,
        seek_bar: &gtk::Scale,
        vol_bar: &gtk::Scale,
        vol_btn: &gtk::Button,
        loop_btn: &gtk::ToggleButton,
    ) {
        play_btn.connect_clicked({
            let v = self.clone();
            move |_| v.toggle_play()
        });
        seek_bar.connect_change_value({
            let v = self.clone();
            move |_, _, val| {
                if let Some(stream) = v.media_stream.borrow().as_ref() {
                    stream.seek(val as i64);
                }
                glib::Propagation::Proceed
            }
        });
        vol_bar.connect_change_value({
            let v = self.clone();
            move |_, _, val| {
                if let Some(stream) = v.media_stream.borrow().as_ref() {
                    stream.set_volume(val.clamp(0.0, 1.0));
                }
                glib::Propagation::Proceed
            }
        });
        vol_btn.connect_clicked({
            let v = self.clone();
            move |btn| {
                if let Some(stream) = v.media_stream.borrow().as_ref() {
                    let muted = !stream.is_muted();
                    stream.set_muted(muted);
                    btn.set_icon_name(if muted {
                        "audio-volume-muted-symbolic"
                    } else {
                        "audio-volume-high-symbolic"
                    });
                }
            }
        });
        loop_btn.connect_toggled({
            let v = self.clone();
            move |btn| {
                if let Some(stream) = v.media_stream.borrow().as_ref() {
                    stream.set_loop(btn.is_active());
                }
            }
        });
    }

    fn toggle_play(&self) {
        if let Some(stream) = self.media_stream.borrow().as_ref() {
            if stream.is_playing() {
                stream.pause();
            } else {
                stream.play();
            }
        }
    }

    fn wire_stage_gestures(self: &Rc<Self>, video_overlay: &gtk::Overlay) {
        // Double-click toggles 1:1 zoom on the image.
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_pressed({
            let v = self.clone();
            move |gesture, n_press, _, _| {
                if n_press == 1 {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                } else if n_press == 2 {
                    v.zoom_step(*v.zoom_level.borrow() == 0.0);
                }
            }
        });
        self.image_scrolled_window.add_controller(click);

        // Click on the video toggles playback.
        let vclick = gtk::GestureClick::new();
        vclick.connect_pressed({
            let v = self.clone();
            move |gesture, n, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if n == 1 {
                    v.toggle_play();
                }
            }
        });
        video_overlay.add_controller(vclick);
    }

    fn wire_keyboard(self: &Rc<Self>) {
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let v = self.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if !v.is_open() {
                return glib::Propagation::Proceed;
            }
            match keyval {
                gtk::gdk::Key::Escape => {
                    v.handle_escape();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::f | gtk::gdk::Key::F => {
                    v.toggle_fullscreen();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::i | gtk::gdk::Key::I => {
                    toggle_info_panel(&v.info_revealer);
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::space => {
                    if v.media_stack.visible_child_name().as_deref() == Some("video") {
                        v.toggle_play();
                    }
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Left => {
                    v.prev();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Right => {
                    v.next();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.overlay.add_controller(key_ctrl);
    }
}

// ── Construction helpers ───────────────────────────────────────────────────

struct InfoBits {
    filename: gtk::Label,
    type_: gtk::Label,
    added: gtk::Label,
    modified: gtk::Label,
    dimensions: gtk::Label,
    duration: gtk::Label,
    duration_row: gtk::Box,
    folder: gtk::Label,
    source: gtk::Label,
    tags_flow: gtk::FlowBox,
}

fn info_row(parent: &gtk::Box, label: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["info-row"])
        .spacing(16)
        .build();
    let key = gtk::Label::builder()
        .label(label)
        .css_classes(["label"])
        .xalign(0.0)
        .width_request(96)
        .valign(gtk::Align::Start)
        .build();
    let value = gtk::Label::builder()
        .css_classes(["value"])
        .xalign(0.0)
        .hexpand(true)
        .wrap(true)
        .selectable(true)
        .build();
    row.append(&key);
    row.append(&value);
    parent.append(&row);
    (row, value)
}

fn build_info_panel() -> (gtk::Revealer, InfoBits) {
    let info_page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(8)
        .build();
    let (_, filename) = info_row(&info_page, "File name");
    let (_, type_) = info_row(&info_page, "Type");
    let (_, added) = info_row(&info_page, "Added");
    let (_, modified) = info_row(&info_page, "Modified");
    let (_, dimensions) = info_row(&info_page, "Dimensions");
    let (duration_row, duration) = info_row(&info_page, "Duration");
    duration_row.set_visible(false);
    let (_, folder) = info_row(&info_page, "Folder");
    let (_, source) = info_row(&info_page, "Source");

    let info_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&info_page)
        .build();

    let tags_flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .row_spacing(8)
        .column_spacing(8)
        .homogeneous(false)
        .margin_top(8)
        .build();
    let tags_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&tags_flow)
        .build();

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled(&info_scrolled, Some("info"), "Info");
    view_stack.add_titled(&tags_scrolled, Some("tags"), "Tags");
    let switcher = adw::ViewSwitcher::builder()
        .stack(&view_stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["info-panel"])
        .spacing(8)
        .build();
    panel.append(&switcher);
    panel.append(&view_stack);

    // A collapsed SlideLeft revealer still reserves its child's width here, so
    // it is also hidden (`visible=false`) when closed; the toggle flips both so
    // a closed panel reserves zero width and the media fills the full stage.
    let info_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideLeft)
        .transition_duration(160)
        .child(&panel)
        .halign(gtk::Align::End)
        .valign(gtk::Align::Fill)
        .vexpand(true)
        .reveal_child(false)
        .visible(false)
        .build();

    (
        info_revealer,
        InfoBits {
            filename,
            type_,
            added,
            modified,
            dimensions,
            duration,
            duration_row,
            folder,
            source,
            tags_flow,
        },
    )
}

#[allow(clippy::type_complexity)]
fn build_topbar() -> (
    gtk::Box,
    gtk::Label,
    gtk::Button,
    gtk::Button,
    gtk::MenuButton,
    gtk::Button,
    gtk::Button,
    gtk::Button,
    gtk::Button,
) {
    let topbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["viewer-topbar"])
        .spacing(12)
        .build();

    let brand = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    brand.append(
        &gtk::Image::builder()
            .icon_name("weather-clear-night-symbolic")
            .pixel_size(24)
            .build(),
    );
    brand.append(
        &gtk::Label::builder()
            .label("Vesper")
            .css_classes(["title-1"])
            .build(),
    );
    topbar.append(&brand);

    let breadcrumb = gtk::Label::builder()
        .css_classes(["breadcrumb"])
        .hexpand(true)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    topbar.append(&breadcrumb);

    let panel_toggle = flat_icon("sidebar-show-right-symbolic", "Toggle info panel");
    let fullscreen_btn = flat_icon("view-fullscreen-symbolic", "Toggle fullscreen");

    let menu_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let menu_open = menu_row(&menu_box, "Open externally");
    let menu_reveal = menu_row(&menu_box, "Reveal in Folder");
    let menu_copy = menu_row(&menu_box, "Copy Path");
    let menu_popover = gtk::Popover::builder().child(&menu_box).build();
    for btn in [&menu_open, &menu_reveal, &menu_copy] {
        let popover = menu_popover.clone();
        btn.connect_clicked(move |_| popover.popdown());
    }
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .css_classes(["flat"])
        .popover(&menu_popover)
        .tooltip_text("More")
        .build();

    let close_btn = flat_icon("window-close-symbolic", "Close viewer");

    topbar.append(&panel_toggle);
    topbar.append(&fullscreen_btn);
    topbar.append(&menu_btn);
    topbar.append(&close_btn);

    (
        topbar,
        breadcrumb,
        panel_toggle,
        fullscreen_btn,
        menu_btn,
        close_btn,
        menu_open,
        menu_reveal,
        menu_copy,
    )
}

/// Opens or closes the read-only Info/Tags panel. Flips both `visible` (so a
/// closed panel reserves zero width) and `reveal-child` (for the slide when
/// opening).
fn toggle_info_panel(revealer: &gtk::Revealer) {
    let show = !revealer.reveals_child();
    if show {
        revealer.set_visible(true);
        revealer.set_reveal_child(true);
    } else {
        revealer.set_reveal_child(false);
        revealer.set_visible(false);
    }
}

fn build_filename_pill() -> (gtk::Box, gtk::Label, gtk::Label) {
    let pill = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["filename-pill", "osd"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Start)
        .spacing(12)
        .build();
    let name = gtk::Label::builder().css_classes(["body-strong"]).build();
    let position = gtk::Label::builder()
        .css_classes(["position", "numeric"])
        .build();
    pill.append(&name);
    pill.append(&position);
    (pill, name, position)
}

#[allow(clippy::type_complexity)]
fn build_zoom_controls() -> (
    gtk::Box,
    gtk::Button,
    gtk::Button,
    gtk::Label,
    gtk::Button,
    gtk::Button,
) {
    let controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["zoom-controls", "osd"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .spacing(4)
        .build();
    let fit = flat_icon("zoom-fit-best-symbolic", "Fit to window");
    let out = flat_icon("zoom-out-symbolic", "Zoom out");
    let label = gtk::Label::builder()
        .label("Fit")
        .css_classes(["numeric"])
        .width_request(48)
        .build();
    let in_ = flat_icon("zoom-in-symbolic", "Zoom in");
    let fs = flat_icon("view-fullscreen-symbolic", "Toggle fullscreen");
    controls.append(&fit);
    controls.append(&out);
    controls.append(&label);
    controls.append(&in_);
    controls.append(&fs);
    (controls, fit, out, label, in_, fs)
}

#[allow(clippy::type_complexity)]
fn build_video_controls() -> (
    gtk::Box,
    gtk::Button,
    gtk::Scale,
    gtk::Label,
    gtk::Button,
    gtk::Scale,
    gtk::ToggleButton,
) {
    let box_ = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["osd", "zoom-controls"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .margin_bottom(80)
        .spacing(8)
        .build();
    let play = gtk::Button::builder()
        .icon_name("media-playback-pause-symbolic")
        .css_classes(["flat"])
        .build();
    let seek = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&gtk::Adjustment::new(
            0.0, 0.0, 1.0, 100000.0, 500000.0, 0.0,
        ))
        .draw_value(false)
        .width_request(280)
        .build();
    let time = gtk::Label::builder()
        .label("0:00 / 0:00")
        .css_classes(["numeric"])
        .build();
    let vol_btn = gtk::Button::builder()
        .icon_name("audio-volume-high-symbolic")
        .css_classes(["flat"])
        .build();
    let vol = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&gtk::Adjustment::new(1.0, 0.0, 1.0, 0.1, 0.1, 0.0))
        .draw_value(false)
        .width_request(90)
        .build();
    let loop_btn = gtk::ToggleButton::builder()
        .icon_name("media-playlist-repeat-symbolic")
        .css_classes(["flat"])
        .active(true)
        .build();
    box_.append(&play);
    box_.append(&time);
    box_.append(&seek);
    box_.append(&vol_btn);
    box_.append(&vol);
    box_.append(&loop_btn);
    (box_, play, seek, time, vol_btn, vol, loop_btn)
}

fn flat_icon(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .css_classes(["flat"])
        .build();
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

fn menu_row(parent: &gtk::Box, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .css_classes(["flat"])
        .child(
            &gtk::Label::builder()
                .label(label)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .build(),
        )
        .build();
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    parent.append(&button);
    button
}

fn placeholder_box(icon: &str, message: &str) -> gtk::Box {
    let box_ = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .spacing(16)
        .build();
    box_.append(
        &gtk::Image::builder()
            .icon_name(icon)
            .pixel_size(48)
            .css_classes(["placeholder-illustration"])
            .build(),
    );
    box_.append(
        &gtk::Label::builder()
            .label(message)
            .css_classes(["title-2"])
            .build(),
    );
    box_
}

fn launch_path(path: &str, folder: bool) {
    let target = if folder {
        std::path::Path::new(path).parent().map(|p| p.to_path_buf())
    } else {
        Some(std::path::PathBuf::from(path))
    };
    if let Some(target) = target
        && let Ok(uri) = glib::filename_to_uri(&target, None)
    {
        gtk::gio::AppInfo::launch_default_for_uri_async(
            &uri,
            None::<&gtk::gio::AppLaunchContext>,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    tracing::warn!(%error, "failed to launch from viewer menu");
                }
            },
        );
    }
}

/// Human media type from extension + classification (e.g. "JPEG image").
fn media_type_label(filename: &str, is_video: bool) -> String {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_uppercase())
        .unwrap_or_default();
    let kind = if is_video { "video" } else { "image" };
    if ext.is_empty() {
        kind.to_string()
    } else {
        format!("{ext} {kind}")
    }
}

fn parent_folder_name(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Formats a Unix-millisecond timestamp as local `YYYY-MM-DD HH:MM`.
fn format_timestamp(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    glib::DateTime::from_unix_local(ms / 1000)
        .ok()
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// Formats whole seconds as `H:MM:SS` or `M:SS`.
fn format_hms(secs: i64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = secs / 3600;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_type_label_uses_extension_and_kind() {
        assert_eq!(media_type_label("IMG_2048.jpg", false), "JPG image");
        assert_eq!(media_type_label("clip.MP4", true), "MP4 video");
        assert_eq!(media_type_label("noext", false), "image");
    }

    #[test]
    fn parent_folder_is_immediate_directory() {
        assert_eq!(
            parent_folder_name("/media/photos/2023 Japan/IMG.jpg").as_deref(),
            Some("2023 Japan")
        );
    }

    #[test]
    fn hms_formats_hours_and_minutes() {
        assert_eq!(format_hms(8), "0:08");
        assert_eq!(format_hms(74), "1:14");
        assert_eq!(format_hms(3661), "1:01:01");
    }
}
