import re

with open('src/ui/viewer.rs', 'r') as f:
    content = f.read()

# 1. pub selection_model and new fields
content = content.replace('selection_model: gtk::MultiSelection,', 
                          'pub selection_model: gtk::MultiSelection,')
content = content.replace('error_label: gtk::Label,\n}', 
                          'error_label: gtk::Label,\n    loop_btn: gtk::ToggleButton,\n    vol_btn: gtk::Button,\n    vol_bar: gtk::Scale,\n    seek_bar: gtk::Scale,\n}')

# 2. Add to constructor
content = content.replace('info_tags,\n            error_label,\n        });', 
                          'info_tags,\n            error_label,\n            loop_btn: loop_btn.clone(),\n            vol_btn: vol_btn.clone(),\n            vol_bar: vol_bar.clone(),\n            seek_bar: seek_bar.clone(),\n        });')

# 3. Viewer chevrons
old_chevrons = '''        let left_revealer = gtk::Revealer::builder()
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
        overlay.add_controller(motion);'''

new_chevrons = '''        let left_edge_box = gtk::Box::builder().width_request(80).halign(gtk::Align::Start).vexpand(true).build();
        let right_edge_box = gtk::Box::builder().width_request(80).halign(gtk::Align::End).vexpand(true).build();

        let left_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&prev_btn)
            .build();
        left_edge_box.append(&left_revealer);
            
        let right_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .child(&next_btn)
            .build();
        right_edge_box.append(&right_revealer);
            
        overlay.add_overlay(&left_edge_box);
        overlay.add_overlay(&right_edge_box);
        
        let motion_left = gtk::EventControllerMotion::new();
        let left_rev_clone = left_revealer.clone();
        motion_left.connect_enter(move |_, _, _| left_rev_clone.set_reveal_child(true));
        let left_rev_clone = left_revealer.clone();
        motion_left.connect_leave(move |_| left_rev_clone.set_reveal_child(false));
        left_edge_box.add_controller(motion_left);

        let motion_right = gtk::EventControllerMotion::new();
        let right_rev_clone = right_revealer.clone();
        motion_right.connect_enter(move |_, _, _| right_rev_clone.set_reveal_child(true));
        let right_rev_clone = right_revealer.clone();
        motion_right.connect_leave(move |_| right_rev_clone.set_reveal_child(false));
        right_edge_box.add_controller(motion_right);'''

content = content.replace(old_chevrons, new_chevrons)

# 4. click gesture
old_click = '''        click_gesture.connect_pressed(move |gesture, n_press, _, _| {
            if n_press == 2 {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                viewer_clone4.toggle_zoom(*pp_clone3.borrow());
            }
        });'''
new_click = '''        click_gesture.connect_pressed(move |gesture, n_press, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if n_press == 2 {
                viewer_clone4.toggle_zoom(*pp_clone3.borrow());
            }
        });'''
content = content.replace(old_click, new_click)

# 5. loop_btn and volume
old_play = '''                stream.set_loop(true);
                stream.play();'''
new_play = '''                stream.set_loop(self.loop_btn.is_active());
                stream.set_volume(self.vol_bar.value());
                let is_muted = self.vol_btn.icon_name().map(|s| s.as_str() == "audio-volume-muted-symbolic").unwrap_or(false);
                stream.set_muted(is_muted);
                stream.play();'''
content = content.replace(old_play, new_play)

# 6. video_controls_have_focus method
new_method = '''
    pub fn video_controls_have_focus(&self) -> bool {
        self.seek_bar.has_focus() || self.vol_bar.has_focus() || self.vol_btn.has_focus() || self.loop_btn.has_focus() || self.play_btn.has_focus()
    }
}'''
content = content.replace('\n}', new_method)

with open('src/ui/viewer.rs', 'w') as f:
    f.write(content)
