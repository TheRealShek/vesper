//! Virtualized grid-cell factory and the UI-owned decoded-thumbnail LRU.
//!
//! Cells hold only compact display summaries (Arch §5); the grid requests a
//! thumbnail only when a cell binds (visible/near-visible). Loading and failure
//! states use a stable static placeholder — never a spinner-driven layout shift
//! (Visual §6). Selection is GridView-managed; the `.selected` visual state maps
//! to the `gridview > child:selected` pseudo the model sets on the cell.

use crate::events::ChannelSendExt;
use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

struct MemoryEntry {
    path: String,
    texture: gtk::gdk::Texture,
    bytes: usize,
    last_used: u64,
}

/// UI-owned decoded-thumbnail LRU. GTK textures stay on the GTK thread while
/// file reads and decoding are requested through typed backend events.
pub struct ThumbnailMemoryCache {
    entries: HashMap<i64, MemoryEntry>,
    pending: HashSet<(i64, String)>,
    pinned: HashSet<i64>,
    used_bytes: usize,
    clock: u64,
}

impl ThumbnailMemoryCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            pinned: HashSet::new(),
            used_bytes: 0,
            clock: 0,
        }
    }

    fn pin(&mut self, media_id: i64) {
        self.pinned.insert(media_id);
    }

    fn unpin(&mut self, media_id: i64) {
        self.pinned.remove(&media_id);
        self.evict_to_limits();
    }

    fn get(&mut self, media_id: i64, path: &str) -> Option<gtk::gdk::Texture> {
        self.clock = self.clock.saturating_add(1);
        let entry = self.entries.get_mut(&media_id)?;
        if entry.path != path {
            return None;
        }
        entry.last_used = self.clock;
        Some(entry.texture.clone())
    }

    fn begin_load(&mut self, media_id: i64, path: &str) -> bool {
        self.pending.insert((media_id, path.to_string()))
    }

    pub fn insert(&mut self, decoded: crate::events::DecodedThumbnail) -> bool {
        self.pending
            .remove(&(decoded.media_id, decoded.path.clone()));
        let Ok(width) = i32::try_from(decoded.width) else {
            return false;
        };
        let Ok(height) = i32::try_from(decoded.height) else {
            return false;
        };
        let Some(stride) = (decoded.width as usize).checked_mul(4) else {
            return false;
        };
        let byte_count = decoded.pixels.len();
        let bytes = glib::Bytes::from_owned(decoded.pixels);
        let texture = gtk::gdk::MemoryTexture::new(
            width,
            height,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            stride,
        )
        .upcast::<gtk::gdk::Texture>();

        self.clock = self.clock.saturating_add(1);
        if let Some(previous) = self.entries.remove(&decoded.media_id) {
            self.used_bytes = self.used_bytes.saturating_sub(previous.bytes);
        }
        self.used_bytes = self.used_bytes.saturating_add(byte_count);
        self.entries.insert(
            decoded.media_id,
            MemoryEntry {
                path: decoded.path,
                texture,
                bytes: byte_count,
                last_used: self.clock,
            },
        );
        self.evict_to_limits();
        true
    }

    fn evict_to_limits(&mut self) {
        while self.entries.len() > crate::config::THUMBNAIL_MEMORY_ENTRY_LIMIT
            || self.used_bytes > crate::config::THUMBNAIL_MEMORY_BUDGET_BYTES
        {
            let victim = self
                .entries
                .iter()
                .filter(|(media_id, _)| !self.pinned.contains(media_id))
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(media_id, _)| *media_id);
            let Some(victim) = victim else {
                break;
            };
            if let Some(removed) = self.entries.remove(&victim) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.bytes);
            }
        }
    }
}

/// Formats a whole-second duration as `H:MM:SS` or `M:SS`.
fn format_duration(secs: i64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = secs / 3600;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Applies the duration badge for a media item: shown for videos with a known
/// (>= 0) duration, hidden otherwise (Product §2 — an unknown duration shows no
/// badge, never an empty rectangle).
fn apply_duration_badge(badge: &gtk::Label, is_video: bool, secs: i64) {
    if is_video && secs >= 0 {
        badge.set_text(&format_duration(secs));
        badge.set_visible(true);
    } else {
        badge.set_text("");
        badge.set_visible(false);
    }
}

/// Create the grid cell factory with setup, bind, and unbind handlers.
// GTK recycles cell widgets during scroll: `bind` wires fresh data to a recycled
// cell, `unbind` clears stale bindings.
#[allow(clippy::too_many_arguments)]
pub fn create_factory(
    viewer_ref: Rc<RefCell<Option<Rc<crate::ui::viewer::Viewer>>>>,
    selection_model: gtk::MultiSelection,
    selection_anchor: Rc<RefCell<Option<u32>>>,
    selection_history: Rc<RefCell<Vec<u32>>>,
    selection_mode: Rc<RefCell<bool>>,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    thumbnail_cache: Rc<RefCell<ThumbnailMemoryCache>>,
    thumb_tx: tokio::sync::mpsc::Sender<crate::thumbnail::ThumbnailRequest>,
    focused_position: Rc<RefCell<Option<u32>>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let viewer_ref_setup = viewer_ref.clone();
    let selection_model_setup = selection_model.clone();
    let selection_mode_setup = selection_mode.clone();
    let focused_position_setup = focused_position.clone();

    factory.connect_setup(move |_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };

        let overlay = gtk::Overlay::builder().hexpand(true).vexpand(true).build();

        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .css_classes(["media-thumb"])
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();
        overlay.set_child(Some(&picture));

        // Stable static placeholder shown while a thumbnail loads or on failure
        // (Visual §6) — no spinner, no layout shift.
        let placeholder = gtk::Image::builder()
            .icon_name("image-x-generic-symbolic")
            .pixel_size(48)
            .css_classes(["placeholder-illustration"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .build();
        overlay.add_overlay(&placeholder);

        // Lavender check badge, top-right; CSS reveals it on the :selected cell.
        let checkmark = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .css_classes(["cell-check"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .can_target(false)
            .build();
        overlay.add_overlay(&checkmark);

        let hover_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["cell-hover-actions"])
            .valign(gtk::Align::End)
            .can_target(false)
            .spacing(4)
            .build();
        let filename_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        hover_box.append(&filename_label);
        overlay.add_overlay(&hover_box);

        let duration_badge = gtk::Label::builder()
            .css_classes(["cell-duration"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::End)
            .can_target(false)
            .visible(false)
            .build();
        overlay.add_overlay(&duration_badge);

        unsafe {
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
        }

        let aspect_frame = gtk::AspectFrame::builder()
            .xalign(0.5)
            .yalign(0.5)
            .ratio(1.0)
            .obey_child(false)
            .child(&overlay)
            .css_classes(["media-cell"])
            .overflow(gtk::Overflow::Hidden)
            .build();

        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(1);
        let viewer_ref_clone = viewer_ref_setup.clone();
        let sel_model = selection_model_setup.clone();
        let selection_mode = selection_mode_setup.clone();
        let selection_anchor = selection_anchor.clone();
        let selection_history = selection_history.clone();
        let list_item_clone = list_item.clone();

        let sync_anchor_from_history = {
            let selection_anchor = selection_anchor.clone();
            let selection_history = selection_history.clone();
            move || {
                let history = selection_history.borrow();
                *selection_anchor.borrow_mut() = history.last().copied();
            }
        };

        let remove_from_history = {
            let selection_history = selection_history.clone();
            move |pos: u32| {
                let mut history = selection_history.borrow_mut();
                if let Some(idx) = history.iter().position(|&p| p == pos) {
                    history.remove(idx);
                }
            }
        };

        let append_to_history = {
            let selection_history = selection_history.clone();
            move |pos: u32| {
                let mut history = selection_history.borrow_mut();
                if let Some(idx) = history.iter().position(|&p| p == pos) {
                    history.remove(idx);
                }
                history.push(pos);
            }
        };

        let replace_history_with_range = {
            let sel_model = sel_model.clone();
            let selection_history = selection_history.clone();
            move |start: u32, end: u32| {
                let mut history = selection_history.borrow_mut();
                for pos in start..=end {
                    if sel_model.is_selected(pos) {
                        if let Some(idx) = history.iter().position(|&p| p == pos) {
                            history.remove(idx);
                        }
                        history.push(pos);
                    }
                }
            }
        };

        let toggle_select = {
            let sel_model = sel_model.clone();
            let selection_anchor = selection_anchor.clone();
            let sync_anchor_from_history = sync_anchor_from_history.clone();
            let remove_from_history = remove_from_history.clone();
            let append_to_history = append_to_history.clone();
            move |pos: u32| {
                if sel_model.is_selected(pos) {
                    sel_model.unselect_item(pos);
                    remove_from_history(pos);
                    sync_anchor_from_history();
                } else {
                    sel_model.select_item(pos, false);
                    append_to_history(pos);
                    *selection_anchor.borrow_mut() = Some(pos);
                }
            }
        };

        click_gesture.connect_pressed(move |gesture, n_press, _, _| {
            if n_press != 1 {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);

            let state = gesture.current_event_state();
            let is_ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let is_shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let pos = list_item_clone.position();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }

            // In selection mode, or with Ctrl held, a plain click toggles the
            // cell rather than opening the viewer (Product §6).
            if is_ctrl || *selection_mode.borrow() {
                toggle_select(pos);
                return;
            }

            if is_shift {
                let anchor = {
                    let current = *selection_anchor.borrow();
                    match current {
                        Some(a) if sel_model.is_selected(a) => a,
                        _ => {
                            sync_anchor_from_history();
                            selection_anchor.borrow().unwrap_or(pos)
                        }
                    }
                };
                let start = std::cmp::min(anchor, pos);
                let end = std::cmp::max(anchor, pos);
                sel_model.select_range(start, end - start + 1, false);
                replace_history_with_range(start, end);
                append_to_history(pos);
                *selection_anchor.borrow_mut() = Some(pos);
                return;
            }

            // Plain click with an active selection but not in selection mode:
            // clear it, then open the viewer (opening clears selection, Arch §9).
            if sel_model.selection().size() > 0 {
                sel_model.unselect_all();
                selection_history.borrow_mut().clear();
                *selection_anchor.borrow_mut() = None;
            }
            if let Some(v) = viewer_ref_clone.borrow().as_ref() {
                v.open(pos);
            }
        });

        aspect_frame.add_controller(click_gesture);
        list_item.set_child(Some(&aspect_frame));

        // Track the model position of the keyboard-focused cell so the grid key
        // handler can toggle/range-select it (keyboard parity with click).
        if let Some(cell) = aspect_frame.parent() {
            let focus_ctrl = gtk::EventControllerFocus::new();
            let focused_position = focused_position_setup.clone();
            let list_item_focus = list_item.clone();
            focus_ctrl.connect_enter(move |_| {
                let pos = list_item_focus.position();
                if pos != gtk::INVALID_LIST_POSITION {
                    *focused_position.borrow_mut() = Some(pos);
                }
            });
            cell.add_controller(focus_ctrl);
        }
    });

    let app_tx_bind = app_tx.clone();
    let thumbnail_cache_bind = thumbnail_cache.clone();
    let thumb_tx_bind = thumb_tx.clone();
    factory.connect_bind(move |_factory, list_item| {
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
        let filename_label = match unsafe { overlay.steal_data::<gtk::Label>("filename_label") } {
            Some(p) => p,
            None => return,
        };
        let duration_badge = match unsafe { overlay.steal_data::<gtk::Label>("duration_badge") } {
            Some(p) => p,
            None => return,
        };

        let filename: String = media_item.property("filename");
        let media_id: i64 = media_item.property("id");
        let is_video: bool = media_item.property("is-video");
        thumbnail_cache_bind.borrow_mut().pin(media_id);
        app_tx_bind.send_log(crate::events::AppEvent::ThumbnailVisibility {
            media_id,
            visible: true,
        });
        filename_label.set_text(&filename);

        let media_type = if is_video { "Video" } else { "Image" };
        overlay.update_property(&[gtk::accessible::Property::Label(&format!(
            "{media_type} {filename}"
        ))]);

        placeholder.set_icon_name(Some(if is_video {
            "video-x-generic-symbolic"
        } else {
            "image-x-generic-symbolic"
        }));
        let d: i64 = media_item.property("duration-secs");
        apply_duration_badge(&duration_badge, is_video, d);

        let id2 = media_item.connect_notify_local(Some("duration-secs"), {
            let badge = duration_badge.clone();
            move |item, _| {
                let d: i64 = item.property("duration-secs");
                let is_video: bool = item.property("is-video");
                apply_duration_badge(&badge, is_video, d);
            }
        });

        let show_placeholder = {
            let pic = picture.clone();
            let plc = placeholder.clone();
            move || {
                pic.set_visible(false);
                plc.set_visible(true);
            }
        };
        let show_texture = {
            let pic = picture.clone();
            let plc = placeholder.clone();
            let cache = thumbnail_cache_bind.clone();
            let app_tx = app_tx_bind.clone();
            move |media_id: i64, thumb_path: &str| {
                if let Some(texture) = cache.borrow_mut().get(media_id, thumb_path) {
                    pic.set_paintable(Some(&texture));
                } else {
                    pic.set_filename(Some(thumb_path));
                    if cache.borrow_mut().begin_load(media_id, thumb_path) {
                        app_tx.send_log(crate::events::AppEvent::ReadThumbnail {
                            media_id,
                            path: thumb_path.to_string(),
                        });
                    }
                }
                pic.set_visible(true);
                plc.set_visible(false);
            }
        };

        let id1 = media_item.connect_notify_local(Some("thumbnail-path"), {
            let show_placeholder = show_placeholder.clone();
            let show_texture = show_texture.clone();
            move |item, _| {
                let thumb_path: String = item.property("thumbnail-path");
                let media_id: i64 = item.property("id");
                if thumb_path.is_empty() {
                    show_placeholder();
                } else {
                    show_texture(media_id, &thumb_path);
                }
            }
        });

        let thumb_path: String = media_item.property("thumbnail-path");
        if thumb_path.is_empty() {
            show_placeholder();
            // Offline-root media is excluded from hydration/search, so every
            // bound cell is online and may request a thumbnail.
            thumb_tx_bind.send_log(crate::thumbnail::ThumbnailRequest {
                media_id,
                path: std::path::PathBuf::from(media_item.property::<String>("path")),
                media_type: if is_video {
                    crate::events::MediaType::Video
                } else {
                    crate::events::MediaType::Image
                },
                modified_at: media_item.property("modified-at"),
            });
        } else {
            show_texture(media_id, &thumb_path);
        }

        unsafe {
            list_item.set_data("sig_id", id1);
            list_item.set_data("sig_duration_id", id2);
            list_item.set_data("bound_media_id", media_id);
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
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
        }
        let media_id: Option<i64> = unsafe { list_item.steal_data("bound_media_id") };
        if let Some(media_id) = media_id {
            thumbnail_cache.borrow_mut().unpin(media_id);
            app_tx.send_log(crate::events::AppEvent::ThumbnailVisibility {
                media_id,
                visible: false,
            });
        }
    });

    factory
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn duration_formats_minutes_and_hours() {
        assert_eq!(format_duration(14), "0:14");
        assert_eq!(format_duration(42), "0:42");
        assert_eq!(format_duration(3661), "1:01:01");
    }
}
