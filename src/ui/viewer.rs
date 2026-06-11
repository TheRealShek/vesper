use libadwaita::gtk::{self, prelude::*, glib, gio};
use std::rc::Rc;
use std::cell::RefCell;

pub struct Viewer {
    pub dim_bg: gtk::Box,
    pub overlay: gtk::Overlay,
    current_index: RefCell<u32>,
    filter_model: gtk::FilterListModel,
    selection_model: gtk::MultiSelection,
    saved_scroll: RefCell<f64>,
    scrolled_window: gtk::ScrolledWindow,
    media_stack: gtk::Stack,
    pub image_scrolled_window: gtk::ScrolledWindow,
    pub zoom_level: RefCell<f64>,
    picture: gtk::Picture,
    video_picture: gtk::Picture,
    media_stream: RefCell<Option<gtk::MediaStream>>,
    controls_visible: RefCell<bool>,
    video_controls_box: gtk::Box,
    play_btn: gtk::Button,
    time_label: gtk::Label,
    seek_bar: gtk::Scale,
    seek_adj: gtk::Adjustment,
    nav_revealers: Vec<gtk::Revealer>,
    controls_revealer: gtk::Revealer,
    pub info_revealer: gtk::Revealer,
    info_filename: gtk::Label,
    info_path: gtk::Label,
    info_size: gtk::Label,
    info_dim_dur: gtk::Label,
    info_created: gtk::Label,
    info_modified: gtk::Label,
    info_tags: gtk::Label,
}

impl Viewer {
    pub fn new(
        filter_model: gtk::FilterListModel,
        selection_model: gtk::MultiSelection,
        scrolled_window: gtk::ScrolledWindow,
    ) -> Rc<Self> {
        let dim_bg = gtk::Box::builder()
            .css_classes(["viewer-bg"])
            .visible(false)
            .build();
            
        let overlay = gtk::Overlay::builder()
            .css_classes(["viewer-overlay"])
            .visible(false)
            .build();
            
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
        
        let video_controls_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["video-controls", "osd"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(24)
            .spacing(8)
            .build();
            
        let play_btn = gtk::Button::builder().icon_name("media-playback-pause-symbolic").build();
        
        let seek_adj = gtk::Adjustment::new(0.0, 0.0, 1.0, 100000.0, 500000.0, 0.0);
        let seek_bar = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&seek_adj)
            .draw_value(false)
            .width_request(300)
            .build();
            
        let time_label = gtk::Label::new(Some("0:00 / 0:00"));
        
        let vol_btn = gtk::Button::builder().icon_name("audio-volume-high-symbolic").build();
        let vol_adj = gtk::Adjustment::new(1.0, 0.0, 1.0, 0.1, 0.1, 0.0);
        let vol_bar = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&vol_adj)
            .draw_value(false)
            .width_request(100)
            .build();
            
        let loop_btn = gtk::ToggleButton::builder()
            .icon_name("media-playlist-repeat-symbolic")
            .active(true)
            .build();
            
        let fs_btn = gtk::Button::builder().icon_name("view-fullscreen-symbolic").build();

        video_controls_box.append(&play_btn);
        video_controls_box.append(&time_label);
        video_controls_box.append(&seek_bar);
        video_controls_box.append(&vol_btn);
        video_controls_box.append(&vol_bar);
        video_controls_box.append(&loop_btn);
        video_controls_box.append(&fs_btn);
        
        let controls_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&video_controls_box)
            .reveal_child(true)
            .build();
            
        video_overlay.add_overlay(&controls_revealer);
        
        let media_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        media_stack.add_named(&image_scrolled_window, Some("image"));
        media_stack.add_named(&video_overlay, Some("video"));
        overlay.set_child(Some(&media_stack));
        
        let prev_btn = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .css_classes(["circular", "osd", "viewer-nav-btn"])
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Start)
            .margin_start(24)
            .build();
            
        let next_btn = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["circular", "osd", "viewer-nav-btn"])
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .margin_end(24)
            .build();
            
        let left_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&prev_btn)
            .build();
            
        let right_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&next_btn)
            .build();
            
        overlay.add_overlay(&left_revealer);
        overlay.add_overlay(&right_revealer);
        
        let motion = gtk::EventControllerMotion::new();
        let left_rev_clone = left_revealer.clone();
        let right_rev_clone = right_revealer.clone();
        motion.connect_enter(move |_, _, _| {
            left_rev_clone.set_reveal_child(true);
            right_rev_clone.set_reveal_child(true);
        });
        let left_rev_clone = left_revealer.clone();
        let right_rev_clone = right_revealer.clone();
        motion.connect_leave(move |_| {
            left_rev_clone.set_reveal_child(false);
            right_rev_clone.set_reveal_child(false);
        });
        overlay.add_controller(motion);
        
        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["circular", "osd"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .margin_top(24)
            .margin_end(24)
            .build();
            
        let close_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&close_btn)
            .build();
        overlay.add_overlay(&close_revealer);
        
        let info_btn = gtk::Button::builder()
            .icon_name("dialog-information-symbolic")
            .css_classes(["circular", "osd"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .margin_top(24)
            .margin_end(80)
            .build();
            
        let info_btn_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&info_btn)
            .build();
        overlay.add_overlay(&info_btn_revealer);
        
        let motion2 = gtk::EventControllerMotion::new();
        let close_rev_clone = close_revealer.clone();
        let info_btn_rev_clone = info_btn_revealer.clone();
        motion2.connect_enter(move |_, _, _| { 
            close_rev_clone.set_reveal_child(true); 
            info_btn_rev_clone.set_reveal_child(true);
        });
        let close_rev_clone = close_revealer.clone();
        let info_btn_rev_clone = info_btn_revealer.clone();
        motion2.connect_leave(move |_| { 
            close_rev_clone.set_reveal_child(false); 
            info_btn_rev_clone.set_reveal_child(false);
        });
        overlay.add_controller(motion2);
        
        let info_panel = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["info-panel", "card", "osd"])
            .width_request(300)
            .margin_top(24)
            .margin_bottom(24)
            .margin_end(24)
            .spacing(12)
            .build();
            
        let info_filename = gtk::Label::builder().halign(gtk::Align::Start).wrap(true).css_classes(["title-3"]).build();
        let info_path = gtk::Label::builder().halign(gtk::Align::Start).wrap(true).css_classes(["dim-label"]).build();
        let info_size = gtk::Label::builder().halign(gtk::Align::Start).build();
        let info_dim_dur = gtk::Label::builder().halign(gtk::Align::Start).build();
        let info_created = gtk::Label::builder().halign(gtk::Align::Start).build();
        let info_modified = gtk::Label::builder().halign(gtk::Align::Start).build();
        let info_tags = gtk::Label::builder().halign(gtk::Align::Start).wrap(true).build();
        
        let add_row = |b: &gtk::Box, title: &str, label: &gtk::Label| {
            let row = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).build();
            let header = gtk::Label::builder().label(title).halign(gtk::Align::Start).css_classes(["dim-label", "caption"]).build();
            row.append(&header);
            row.append(label);
            b.append(&row);
        };
        
        info_panel.append(&info_filename);
        info_panel.append(&info_path);
        add_row(&info_panel, "Size", &info_size);
        add_row(&info_panel, "Dimensions / Duration", &info_dim_dur);
        add_row(&info_panel, "Created", &info_created);
        add_row(&info_panel, "Modified", &info_modified);
        add_row(&info_panel, "Tags", &info_tags);
        
        let info_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .child(&info_panel)
            .halign(gtk::Align::End)
            .build();
        overlay.add_overlay(&info_revealer);
        
        let info_rev_clone = info_revealer.clone();
        info_btn.connect_clicked(move |_| {
            info_rev_clone.set_reveal_child(!info_rev_clone.reveals_child());
        });
        
        let nav_revealers = vec![
            left_revealer.clone(),
            right_revealer.clone(),
            close_revealer.clone(),
            info_btn_revealer.clone(),
        ];
        
        let viewer = Rc::new(Self {
            dim_bg,
            overlay,
            current_index: RefCell::new(0),
            filter_model,
            selection_model,
            saved_scroll: RefCell::new(0.0),
            scrolled_window,
            media_stack,
            image_scrolled_window: image_scrolled_window.clone(),
            picture,
            video_picture,
            media_stream: RefCell::new(None),
            zoom_level: RefCell::new(0.0),
            controls_visible: RefCell::new(true),
            video_controls_box,
            play_btn: play_btn.clone(),
            time_label: time_label.clone(),
            seek_bar: seek_bar.clone(),
            seek_adj: seek_adj.clone(),
            nav_revealers,
            controls_revealer: controls_revealer.clone(),
            info_revealer: info_revealer.clone(),
            info_filename,
            info_path,
            info_size,
            info_dim_dur,
            info_created,
            info_modified,
            info_tags,
        });
        
        // Video Controls logic
        let v_clone_play = viewer.clone();
        play_btn.connect_clicked(move |_| {
            if let Some(stream) = v_clone_play.media_stream.borrow().as_ref() {
                if stream.is_playing() {
                    stream.pause();
                } else {
                    stream.play();
                }
            }
        });
        
        let v_clone_vol = viewer.clone();
        vol_bar.connect_change_value(move |_, _, val| {
            if let Some(stream) = v_clone_vol.media_stream.borrow().as_ref() {
                stream.set_volume(val.clamp(0.0, 1.0));
            }
            glib::Propagation::Proceed
        });
        
        let v_clone_seek = viewer.clone();
        seek_bar.connect_change_value(move |_, _, val| {
            if let Some(stream) = v_clone_seek.media_stream.borrow().as_ref() {
                stream.seek(val as i64);
            }
            glib::Propagation::Proceed
        });
        
        let v_clone_loop = viewer.clone();
        loop_btn.connect_toggled(move |btn| {
            if let Some(stream) = v_clone_loop.media_stream.borrow().as_ref() {
                stream.set_loop(btn.is_active());
            }
        });
        
        let v_clone_fs = viewer.clone();
        fs_btn.connect_clicked(move |_| {
            v_clone_fs.toggle_fullscreen();
        });
        
        let close_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        
        let click_play = gtk::GestureClick::new();
        let v_clone_click = viewer.clone();
        let close_timer_clone1 = close_timer.clone();
        click_play.connect_pressed(move |gesture, n_press, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(tid) = close_timer_clone1.borrow_mut().take() {
                tid.remove();
            }
            if n_press == 1 {
                if let Some(stream) = v_clone_click.media_stream.borrow().as_ref() {
                    if stream.is_playing() {
                        stream.pause();
                    } else {
                        stream.play();
                    }
                }
            }
        });
        video_overlay.add_controller(click_play);
        
        // Setup existing interactions
        let pointer_pos = Rc::new(RefCell::new((0.0, 0.0)));
        let pointer_motion = gtk::EventControllerMotion::new();
        let pp_clone = pointer_pos.clone();
        pointer_motion.connect_motion(move |_, x, y| {
            *pp_clone.borrow_mut() = (x, y);
        });
        image_scrolled_window.add_controller(pointer_motion);

        let scroll_ctrl = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let viewer_clone_zoom = viewer.clone();
        let pp_clone2 = pointer_pos.clone();
        scroll_ctrl.connect_scroll(move |_, _, dy| {
            viewer_clone_zoom.handle_scroll(dy, *pp_clone2.borrow());
            glib::Propagation::Stop
        });
        image_scrolled_window.add_controller(scroll_ctrl);
        
        let drag_gesture = gtk::GestureDrag::new();
        drag_gesture.set_button(0);
        let viewer_clone2 = viewer.clone();
        let pan_start_scroll = Rc::new(RefCell::new((0.0, 0.0)));
        let pss_clone = pan_start_scroll.clone();
        drag_gesture.connect_drag_begin(move |_, _, _| {
            let hadj = viewer_clone2.image_scrolled_window.hadjustment();
            let vadj = viewer_clone2.image_scrolled_window.vadjustment();
            *pss_clone.borrow_mut() = (hadj.value(), vadj.value());
        });
        
        let viewer_clone3 = viewer.clone();
        let pss_clone2 = pan_start_scroll.clone();
        drag_gesture.connect_drag_update(move |_, dx, dy| {
            if *viewer_clone3.zoom_level.borrow() > 0.0 {
                let (sx, sy) = *pss_clone2.borrow();
                viewer_clone3.image_scrolled_window.hadjustment().set_value(sx - dx);
                viewer_clone3.image_scrolled_window.vadjustment().set_value(sy - dy);
            }
        });
        image_scrolled_window.add_controller(drag_gesture);
        
        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(0);
        let viewer_clone4 = viewer.clone();
        let pp_clone3 = pointer_pos.clone();
        let close_timer_clone2 = close_timer.clone();
        click_gesture.connect_pressed(move |gesture, n_press, _, _| {
            if n_press == 2 {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if let Some(tid) = close_timer_clone2.borrow_mut().take() {
                    tid.remove();
                }
                viewer_clone4.toggle_zoom(*pp_clone3.borrow());
            }
        });
        image_scrolled_window.add_controller(click_gesture);
        
        let click_close = gtk::GestureClick::new();
        let v_clone_close = viewer.clone();
        let close_timer_clone3 = close_timer.clone();
        click_close.connect_released(move |_, n, _, _| {
            if n == 1 {
                let v = v_clone_close.clone();
                let tid = glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                    v.close();
                    glib::ControlFlow::Break
                });
                *close_timer_clone3.borrow_mut() = Some(tid);
            }
        });
        viewer.overlay.add_controller(click_close);
        
        // Fullscreen and Info key shortcuts
        let key_ctrl = gtk::EventControllerKey::new();
        let viewer_clone_f = viewer.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if viewer_clone_f.is_open() {
                if keyval == gtk::gdk::Key::f || keyval == gtk::gdk::Key::F {
                    viewer_clone_f.toggle_fullscreen();
                    return glib::Propagation::Stop;
                }
                if keyval == gtk::gdk::Key::i || keyval == gtk::gdk::Key::I {
                    let rev = &viewer_clone_f.info_revealer;
                    rev.set_reveal_child(!rev.reveals_child());
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        viewer.overlay.add_controller(key_ctrl);
        
        let v_clone = viewer.clone();
        prev_btn.connect_clicked(move |_| v_clone.prev());
        
        let v_clone2 = viewer.clone();
        next_btn.connect_clicked(move |_| v_clone2.next());
        
        let v_clone3 = viewer.clone();
        close_btn.connect_clicked(move |_| v_clone3.close());
        
        viewer
    }
    
    pub fn is_open(&self) -> bool {
        self.overlay.is_visible()
    }
    
    pub fn open(&self, position: u32) {
        let n_items = self.filter_model.n_items();
        if position >= n_items { return; }
        
        *self.current_index.borrow_mut() = position;
        
        let vadj = self.scrolled_window.vadjustment();
        *self.saved_scroll.borrow_mut() = vadj.value();
        
        self.load_item(position);
        
        self.dim_bg.set_visible(true);
        self.overlay.set_visible(true);
        
        let dim_bg = self.dim_bg.clone();
        let overlay = self.overlay.clone();
        glib::idle_add_local(move || {
            dim_bg.add_css_class("open");
            overlay.add_css_class("open");
            glib::ControlFlow::Break
        });
    }
    
    pub fn close(&self) {
        self.dim_bg.remove_css_class("open");
        self.overlay.remove_css_class("open");
        
        if let Some(stream) = self.media_stream.borrow().as_ref() {
            stream.pause();
        }
        *self.media_stream.borrow_mut() = None;
        self.video_picture.set_paintable(None::<&gtk::gdk::Paintable>);
        
        let dim_bg = self.dim_bg.clone();
        let overlay = self.overlay.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            dim_bg.set_visible(false);
            overlay.set_visible(false);
            glib::ControlFlow::Break
        });
        
        let vadj = self.scrolled_window.vadjustment();
        vadj.set_value(*self.saved_scroll.borrow());
        
        let pos = *self.current_index.borrow();
        self.selection_model.select_item(pos, true);
    }
    
    pub fn next(&self) {
        let n_items = self.filter_model.n_items();
        let mut idx = *self.current_index.borrow() + 1;
        if idx >= n_items { idx = 0; }
        
        *self.current_index.borrow_mut() = idx;
        self.load_item(idx);
    }
    
    pub fn prev(&self) {
        let n_items = self.filter_model.n_items();
        let mut idx = *self.current_index.borrow();
        if idx == 0 {
            idx = n_items.saturating_sub(1);
        } else {
            idx -= 1;
        }
        
        *self.current_index.borrow_mut() = idx;
        self.load_item(idx);
    }
    
    fn format_time(mut us: i64) -> String {
        us /= 1_000_000;
        let secs = us % 60;
        let mins = (us / 60) % 60;
        let hours = us / 3600;
        if hours > 0 {
            format!("{}:{:02}:{:02}", hours, mins, secs)
        } else {
            format!("{}:{:02}", mins, secs)
        }
    }
    
    fn load_item(&self, position: u32) {
        if let Some(obj) = self.filter_model.item(position) {
            let media_item = obj.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
            let path: String = media_item.property("path");
            let filename: String = media_item.property("filename");
            
            *self.zoom_level.borrow_mut() = 0.0;
            self.apply_zoom();
            
            let file = gio::File::for_path(&path);
            let is_video = filename.ends_with(".mp4") || filename.ends_with(".webm") || filename.ends_with(".mkv");
            
            if let Ok(info) = file.query_info("standard::size,time::modified,time::created", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                let size = info.size(); // bytes
                let size_mb = size as f64 / 1_048_576.0;
                self.info_size.set_text(&format!("{:.1} MB", size_mb));
                
                let mtime = info.modification_date_time().map(|d| d.format("%Y-%m-%d %H:%M:%S").ok().map(|s| s.to_string()).unwrap_or_default()).unwrap_or_default();
                self.info_modified.set_text(&mtime);
                
                let ctime_epoch = info.attribute_uint64("time::created");
                let ctime = if ctime_epoch > 0 {
                    glib::DateTime::from_unix_local(ctime_epoch as i64)
                        .ok()
                        .and_then(|d| d.format("%Y-%m-%d %H:%M:%S").ok().map(|s| s.to_string()))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                self.info_created.set_text(&ctime);
            }
            
            self.info_filename.set_text(&filename);
            self.info_path.set_text(&path);
            
            let tags: String = media_item.property("tags");
            self.info_tags.set_text(if tags.is_empty() { "None" } else { &tags });

            if !is_video {
                if let Ok(tex) = gtk::gdk::Texture::from_file(&file) {
                    self.info_dim_dur.set_text(&format!("{} x {}", tex.width(), tex.height()));
                } else {
                    self.info_dim_dur.set_text("Unknown");
                }
            } else {
                let dur: i64 = media_item.property("duration-secs");
                if dur > 0 {
                    self.info_dim_dur.set_text(&Self::format_time(dur * 1_000_000));
                } else {
                    self.info_dim_dur.set_text("Unknown");
                }
            }
            
            if is_video {
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
                    let ts = s.timestamp();
                    let dur = s.duration();
                    time_label.set_text(&format!("{} / {}", Self::format_time(ts), Self::format_time(dur)));
                    seek_adj.set_value(ts as f64);
                });
                
                let seek_adj = self.seek_adj.clone();
                stream.connect_duration_notify(move |s| {
                    seek_adj.set_upper(s.duration() as f64);
                });
                
                stream.set_loop(true);
                stream.play();
                
                self.video_picture.set_paintable(Some(&stream));
                *self.media_stream.borrow_mut() = Some(stream.upcast());
                self.media_stack.set_visible_child_name("video");
            } else {
                if let Some(stream) = self.media_stream.borrow().as_ref() {
                    stream.pause();
                }
                *self.media_stream.borrow_mut() = None;
                self.picture.set_file(Some(&file));
                self.media_stack.set_visible_child_name("image");
            }
        }
    }
    
    pub fn toggle_fullscreen(&self) {
        let is_visible = !*self.controls_visible.borrow();
        *self.controls_visible.borrow_mut() = is_visible;
        
        self.controls_revealer.set_reveal_child(is_visible);
        for rev in &self.nav_revealers {
            // Note: motion controllers manage left/right navigation, so fullscreen might be overriden by hover.
            // Wait, we should probably toggle visibility directly or let revealer handle it?
            // Since hover events are active, if we set the revealer's child to visible(false), it won't show.
            if let Some(child) = rev.child() {
                child.set_visible(is_visible);
            }
        }
        
        // Dim bg opacity? We could add a class to dim_bg to make it totally black.
        if is_visible {
            self.dim_bg.remove_css_class("fullscreen");
        } else {
            self.dim_bg.add_css_class("fullscreen");
        }
    }

    pub fn toggle_zoom(&self, pointer: (f64, f64)) {
        if *self.zoom_level.borrow() > 0.0 {
            *self.zoom_level.borrow_mut() = 0.0;
            self.apply_zoom();
        } else {
            self.zoom_to(1.0, pointer);
        }
    }

    pub fn apply_zoom(&self) {
        let zoom = *self.zoom_level.borrow();
        if zoom == 0.0 {
            self.picture.set_content_fit(gtk::ContentFit::Contain);
            self.picture.set_can_shrink(true);
            self.picture.set_size_request(-1, -1);
        } else {
            if let Some(p) = self.picture.paintable() {
                let w = p.intrinsic_width() as f64;
                let h = p.intrinsic_height() as f64;
                if w > 0.0 && h > 0.0 {
                    self.picture.set_content_fit(gtk::ContentFit::Fill);
                    self.picture.set_can_shrink(false);
                    self.picture.set_size_request((w * zoom) as i32, (h * zoom) as i32);
                }
            }
        }
    }
    
    pub fn zoom_to(&self, target_zoom: f64, pointer: (f64, f64)) {
        let paintable = match self.picture.paintable() {
            Some(p) => p,
            None => return,
        };
        
        let w = paintable.intrinsic_width() as f64;
        let h = paintable.intrinsic_height() as f64;
        if w <= 0.0 || h <= 0.0 { return; }
        
        let alloc_w = self.image_scrolled_window.width() as f64;
        let alloc_h = self.image_scrolled_window.height() as f64;
        let fit_zoom = (alloc_w / w).min(alloc_h / h);
        
        let current_zoom = *self.zoom_level.borrow();
        
        let hadj = self.image_scrolled_window.hadjustment();
        let vadj = self.image_scrolled_window.vadjustment();
        
        let (px, py) = pointer;
        let rel_x;
        let rel_y;
        
        if current_zoom == 0.0 {
            let tex_x = (alloc_w - w * fit_zoom) / 2.0;
            let tex_y = (alloc_h - h * fit_zoom) / 2.0;
            rel_x = (px - tex_x) / (w * fit_zoom);
            rel_y = (py - tex_y) / (h * fit_zoom);
        } else {
            rel_x = (px + hadj.value()) / (w * current_zoom);
            rel_y = (py + vadj.value()) / (h * current_zoom);
        }
        
        let final_zoom = if target_zoom <= fit_zoom { 0.0 } else { target_zoom };
        
        *self.zoom_level.borrow_mut() = final_zoom;
        self.apply_zoom();
        
        let new_scroll_x = if final_zoom == 0.0 { 0.0 } else { rel_x * w * final_zoom - px };
        let new_scroll_y = if final_zoom == 0.0 { 0.0 } else { rel_y * h * final_zoom - py };
        
        glib::idle_add_local(move || {
            hadj.set_value(new_scroll_x);
            vadj.set_value(new_scroll_y);
            glib::ControlFlow::Break
        });
    }
    
    pub fn handle_scroll(&self, dy: f64, pointer: (f64, f64)) {
        let paintable = match self.picture.paintable() {
            Some(p) => p,
            None => return,
        };
        
        let w = paintable.intrinsic_width() as f64;
        let h = paintable.intrinsic_height() as f64;
        if w <= 0.0 || h <= 0.0 { return; }
        
        let alloc_w = self.image_scrolled_window.width() as f64;
        let alloc_h = self.image_scrolled_window.height() as f64;
        let fit_zoom = (alloc_w / w).min(alloc_h / h);
        
        let current_zoom = *self.zoom_level.borrow();
        let base_zoom = if current_zoom == 0.0 { fit_zoom } else { current_zoom };
        
        let zoom_step = 1.15;
        let new_zoom = if dy < 0.0 {
            base_zoom * zoom_step
        } else {
            base_zoom / zoom_step
        };
        
        self.zoom_to(new_zoom, pointer);
    }
}
