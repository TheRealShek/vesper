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
    picture: gtk::Picture,
    video: gtk::Video,
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
            
        let video = gtk::Video::builder()
            .autoplay(true)
            .build();
            
        let media_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        media_stack.add_named(&picture, Some("image"));
        media_stack.add_named(&video, Some("video"));
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
        
        let motion2 = gtk::EventControllerMotion::new();
        let close_rev_clone = close_revealer.clone();
        motion2.connect_enter(move |_, _, _| { close_rev_clone.set_reveal_child(true); });
        let close_rev_clone = close_revealer.clone();
        motion2.connect_leave(move |_| { close_rev_clone.set_reveal_child(false); });
        overlay.add_controller(motion2);
        
        let viewer = Rc::new(Self {
            dim_bg,
            overlay,
            current_index: RefCell::new(0),
            filter_model,
            selection_model,
            saved_scroll: RefCell::new(0.0),
            scrolled_window,
            media_stack,
            picture,
            video,
        });
        
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
        
        // Save scroll
        let vadj = self.scrolled_window.vadjustment();
        *self.saved_scroll.borrow_mut() = vadj.value();
        
        self.load_item(position);
        
        self.dim_bg.set_visible(true);
        self.overlay.set_visible(true);
        
        // Add open classes for CSS transitions
        // We delay it slightly so the DOM updates and transition plays
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
        
        // Stop video playback
        self.video.set_file(None::<&gio::File>);
        
        // Let transition finish before hiding
        let dim_bg = self.dim_bg.clone();
        let overlay = self.overlay.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            dim_bg.set_visible(false);
            overlay.set_visible(false);
            glib::ControlFlow::Break
        });
        
        // Restore exact scroll
        let vadj = self.scrolled_window.vadjustment();
        vadj.set_value(*self.saved_scroll.borrow());
        
        // Highlight current cell
        let pos = *self.current_index.borrow();
        self.selection_model.select_item(pos, true);
    }
    
    pub fn next(&self) {
        let n_items = self.filter_model.n_items();
        let mut idx = *self.current_index.borrow() + 1;
        if idx >= n_items { idx = 0; } // wrap
        
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
    
    fn load_item(&self, position: u32) {
        if let Some(obj) = self.filter_model.item(position) {
            let media_item = obj.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
            let path: String = media_item.property("path");
            let filename: String = media_item.property("filename");
            
            let file = gio::File::for_path(&path);
            let is_video = filename.ends_with(".mp4") || filename.ends_with(".webm") || filename.ends_with(".mkv");
            
            if is_video {
                self.video.set_file(Some(&file));
                self.media_stack.set_visible_child_name("video");
            } else {
                self.picture.set_file(Some(&file));
                self.media_stack.set_visible_child_name("image");
            }
        }
    }
}
