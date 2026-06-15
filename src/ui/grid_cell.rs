use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Create the grid cell factory with setup, bind, and unbind handlers.
// GTK recycles cell widgets during scroll. The factory uses bind to wire fresh data to a recycled cell, and unbind to prevent stale data display.
pub fn create_factory(
    viewer_ref: Rc<RefCell<Option<Rc<crate::ui::viewer::Viewer>>>>,
    selection_model: gtk::MultiSelection,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let viewer_ref_setup = viewer_ref.clone();
    let selection_model_setup = selection_model.clone();

    factory.connect_setup(move |_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };

        let overlay = gtk::Overlay::builder().hexpand(true).vexpand(true).build();

        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();
        overlay.set_child(Some(&picture));

        let placeholder = gtk::Image::builder()
            .icon_name("image-x-generic-symbolic")
            .pixel_size(48)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .build();
        overlay.add_overlay(&placeholder);

        let checkmark = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .css_classes(["check-icon"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(8)
            .margin_top(8)
            .build();
        overlay.add_overlay(&checkmark);

        let hover_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["cell-hover-overlay"])
            .valign(gtk::Align::End)
            .spacing(4)
            .build();

        let type_icon = gtk::Image::builder()
            .icon_name("image-x-generic-symbolic")
            .build();
        let filename_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        hover_box.append(&type_icon);
        hover_box.append(&filename_label);
        overlay.add_overlay(&hover_box);

        let duration_badge = gtk::Label::builder()
            .css_classes(["duration-badge", "numeric"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .margin_end(8)
            .margin_bottom(8)
            .visible(false)
            .build();
        overlay.add_overlay(&duration_badge);

        let offline_icon = gtk::Image::builder()
            .icon_name("network-offline-symbolic")
            .pixel_size(48)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .visible(false)
            .build();
        overlay.add_overlay(&offline_icon);

        unsafe {
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("type_icon", type_icon);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
            overlay.set_data("offline_icon", offline_icon);
        }

        let aspect_frame = gtk::AspectFrame::builder()
            .xalign(0.5)
            .yalign(0.5)
            .ratio(1.0)
            .obey_child(false)
            .child(&overlay)
            .css_classes(["card", "media-cell"])
            .overflow(gtk::Overflow::Hidden)
            .build();

        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(1);
        let viewer_ref_clone = viewer_ref_setup.clone();
        let sel_model = selection_model_setup.clone();
        let list_item_clone = list_item.clone();

        click_gesture.connect_pressed(move |gesture, n_press, _, _| {
            if n_press != 1 {
                return;
            }
            let state = gesture.current_event_state();
            let is_ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let is_shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);

            if sel_model.selection().size() > 0 && !is_ctrl && !is_shift {
                let pos = list_item_clone.position();
                if pos != gtk::INVALID_LIST_POSITION
                    && let Some(v) = viewer_ref_clone.borrow().as_ref()
                {
                    v.open(pos);
                }
            }
        });

        aspect_frame.add_controller(click_gesture);

        list_item.set_child(Some(&aspect_frame));
    });

    factory.connect_bind(move |_factory, list_item| {
        // Thumbnail loading is implicitly tied to this bind step because a cell is only visible when bound.
        // Requesting thumbnails eagerly from the model for invisible cells would waste I/O bandwidth and worker slots.
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(media_item) = list_item
            .item()
            .and_downcast::<crate::ui::model::MediaItem>()
        else {
            return;
        };
        let Some(aspect_frame) = list_item.child().and_downcast::<gtk::AspectFrame>() else {
            return;
        };
        let Some(overlay) = aspect_frame.child().and_downcast::<gtk::Overlay>() else {
            return;
        };

        let picture = match unsafe { overlay.steal_data::<gtk::Picture>("picture") } {
            Some(p) => p,
            None => return,
        };
        let placeholder = match unsafe { overlay.steal_data::<gtk::Image>("placeholder") } {
            Some(p) => p,
            None => return,
        };
        let type_icon = match unsafe { overlay.steal_data::<gtk::Image>("type_icon") } {
            Some(p) => p,
            None => return,
        };
        let filename_label = match unsafe { overlay.steal_data::<gtk::Label>("filename_label") } {
            Some(p) => p,
            None => return,
        };
        let duration_badge = match unsafe { overlay.steal_data::<gtk::Label>("duration_badge") } {
            Some(p) => p,
            None => return,
        };
        let offline_icon = match unsafe { overlay.steal_data::<gtk::Image>("offline_icon") } {
            Some(p) => p,
            None => return,
        };

        let filename: String = media_item.property("filename");
        filename_label.set_text(&filename);

        let is_video: bool = media_item.property("is-video");
        let media_type = if is_video { "Video" } else { "Image" };
        overlay.update_property(&[gtk::accessible::Property::Label(&format!(
            "{} {}",
            media_type, filename
        ))]);

        let d: i64 = media_item.property("duration-secs");
        if is_video {
            type_icon.set_icon_name(Some("video-x-generic-symbolic"));
            placeholder.set_icon_name(Some("video-x-generic-symbolic"));
            if d >= 0 {
                let secs = d % 60;
                let mins = (d / 60) % 60;
                let hours = d / 3600;
                if hours > 0 {
                    duration_badge.set_text(&format!("{}:{:02}:{:02}", hours, mins, secs));
                } else {
                    duration_badge.set_text(&format!("{}:{:02}", mins, secs));
                }
            } else {
                duration_badge.set_text("");
            }
            duration_badge.set_visible(true);
        } else {
            type_icon.set_icon_name(Some("image-x-generic-symbolic"));
            placeholder.set_icon_name(Some("image-x-generic-symbolic"));
            duration_badge.set_visible(false);
        }

        let is_offline: bool = media_item.property("is-offline");
        if is_offline {
            overlay.set_opacity(0.4);
            offline_icon.set_visible(true);
        } else {
            overlay.set_opacity(1.0);
            offline_icon.set_visible(false);
        }

        let id2 = media_item.connect_notify_local(Some("duration-secs"), {
            let dbg = duration_badge.clone();
            move |item, _| {
                let d: i64 = item.property("duration-secs");
                if d >= 0 {
                    let secs = d % 60;
                    let mins = (d / 60) % 60;
                    let hours = d / 3600;
                    if hours > 0 {
                        dbg.set_text(&format!("{}:{:02}:{:02}", hours, mins, secs));
                    } else {
                        dbg.set_text(&format!("{}:{:02}", mins, secs));
                    }
                } else {
                    dbg.set_text("");
                }
            }
        });

        let id1 = media_item.connect_notify_local(Some("thumbnail-path"), {
            let pic = picture.clone();
            let plc = placeholder.clone();
            let ovl = overlay.clone();
            move |item, _| {
                let thumb_path: String = item.property("thumbnail-path");
                if thumb_path.is_empty() {
                    pic.set_visible(false);
                    plc.set_visible(true);
                    ovl.add_css_class("loading");
                } else {
                    pic.set_filename(Some(&thumb_path));
                    pic.set_visible(true);
                    plc.set_visible(false);
                    ovl.remove_css_class("loading");
                }
            }
        });

        let thumb_path: String = media_item.property("thumbnail-path");
        if thumb_path.is_empty() {
            picture.set_visible(false);
            placeholder.set_visible(true);
            overlay.add_css_class("loading");
        } else {
            picture.set_filename(Some(&thumb_path));
            picture.set_visible(true);
            placeholder.set_visible(false);
            overlay.remove_css_class("loading");
        }

        let id3 = media_item.connect_notify_local(Some("is-offline"), {
            let ov = overlay.clone();
            let off = offline_icon.clone();
            move |item, _| {
                let is_offline: bool = item.property("is-offline");
                if is_offline {
                    ov.set_opacity(0.4);
                    off.set_visible(true);
                } else {
                    ov.set_opacity(1.0);
                    off.set_visible(false);
                }
            }
        });

        unsafe {
            list_item.set_data("sig_id", id1);
            list_item.set_data("sig_duration_id", id2);
            list_item.set_data("sig_offline_id", id3);
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("type_icon", type_icon);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
            overlay.set_data("offline_icon", offline_icon);
        }
    });

    factory.connect_unbind(move |_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let Some(media_item) = list_item
            .item()
            .and_downcast::<crate::ui::model::MediaItem>()
        {
            let sig_id: Option<glib::SignalHandlerId> = unsafe { list_item.steal_data("sig_id") };
            if let Some(id) = sig_id {
                media_item.disconnect(id);
            }
            let sig_duration_id: Option<glib::SignalHandlerId> =
                unsafe { list_item.steal_data("sig_duration_id") };
            if let Some(id) = sig_duration_id {
                media_item.disconnect(id);
            }
            let sig_offline_id: Option<glib::SignalHandlerId> =
                unsafe { list_item.steal_data("sig_offline_id") };
            if let Some(id) = sig_offline_id {
                media_item.disconnect(id);
            }
        }
    });

    factory
}
