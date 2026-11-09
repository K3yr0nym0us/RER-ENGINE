//! Estado compartido, selección y helpers de hit-test.
//!
//! Expande los métodos de este grupo dentro del `impl State` del motor.

#[doc(hidden)]
#[macro_export]
macro_rules! __rer_player_ui_text_core {
    () => {
        pub(crate) fn clear_player_ui_text_interaction(&mut self) {
            self.clear_player_ui_hud_selection();
            self.player_ui_text_editing_id = None;
            self.player_ui_text_caret = 0;
            self.player_ui_text_drag = None;
            self.player_ui_last_text_click = None;
        }

        pub(crate) fn clear_player_ui_hud_selection(&mut self) {
            self.player_ui_selected_text_id = None;
            self.player_ui_selected_button_id = None;
            self.player_ui_selected_image_id = None;
            self.player_ui_selected_object_id = None;
        }

        pub(crate) fn player_ui_caret_blink_visible(&self) -> bool {
            self.player_ui_caret_blink_epoch.elapsed().as_millis() as u64 % CARET_BLINK_PERIOD_MS
                < CARET_BLINK_ON_MS
        }

        fn player_ui_restart_caret_blink(&mut self) {
            self.player_ui_caret_blink_epoch = Instant::now();
        }

        fn player_ui_clamp_caret_for_box(&mut self, text: &str) {
            self.player_ui_text_caret = self.player_ui_text_caret.min(text.chars().count());
        }

        fn player_ui_begin_text_edit(&mut self, id: u32) {
            let char_count = self
                .player_ui_boxes_for_id(id)
                .map(|b| b.text.chars().count())
                .unwrap_or(0);
            self.player_ui_text_editing_id = Some(id);
            self.player_ui_text_caret = char_count;
            self.player_ui_restart_caret_blink();
        }

        pub(crate) fn player_ui_boxes_for_id(&self, id: u32) -> Option<&PlayerUiTextBox> {
            let key = self.player_ui_text_key()?;
            self.player_ui_text_boxes
                .get(&key)
                .and_then(|list| list.iter().find(|b| b.id == id))
        }

        pub(crate) fn player_ui_text_key(&self) -> Option<String> {
            self.player_ui_screen_key()
        }

        pub(crate) fn set_player_ui_edit_context(
            &mut self,
            scope: Option<String>,
            screen_id: Option<String>,
        ) {
            self.player_ui_edit_scope = scope;
            self.player_ui_edit_screen_id = screen_id;
            self.clear_player_ui_text_interaction();
            self.rebuild_player_ui_overlay();
        }

        pub(crate) fn pixel_to_ndc(&self, px: f32, py: f32) -> [f32; 2] {
            rer_engine_core::player_ui::viewport::pixel_to_ndc(
                self.size.width.max(1) as f32,
                self.size.height.max(1) as f32,
                px,
                py,
            )
        }

        fn player_ui_boxes_mut(&mut self) -> Option<&mut Vec<PlayerUiTextBox>> {
            let key = self.player_ui_text_key()?;
            self.player_ui_text_boxes.get_mut(&key)
        }

        fn player_ui_buttons_mut(&mut self) -> Option<&mut Vec<PlayerUiButton>> {
            let key = self.player_ui_text_key()?;
            self.player_ui_buttons.get_mut(&key)
        }

        fn player_ui_images_mut(&mut self) -> Option<&mut Vec<PlayerUiImage>> {
            let key = self.player_ui_text_key()?;
            self.player_ui_images.get_mut(&key)
        }

        fn player_ui_objects_mut(
            &mut self,
        ) -> Option<&mut Vec<rer_engine_core::player_ui::config::PlayerUiObject>> {
            let key = self.player_ui_text_key()?;
            self.player_ui_objects.get_mut(&key)
        }

        pub(crate) fn player_ui_object_locked(&self, id: u32) -> bool {
            self.player_ui_text_key()
                .and_then(|key| self.player_ui_objects.get(&key))
                .and_then(|list| list.iter().find(|o| o.id == id))
                .is_some_and(|o| o.locked)
        }

        fn clear_player_ui_selection_except_image(&mut self) {
            self.player_ui_selected_text_id = None;
            self.player_ui_selected_button_id = None;
            self.player_ui_selected_object_id = None;
            self.player_ui_text_editing_id = None;
        }

        fn clear_player_ui_selection_except_button(&mut self) {
            self.player_ui_selected_text_id = None;
            self.player_ui_selected_image_id = None;
            self.player_ui_selected_object_id = None;
            self.player_ui_text_editing_id = None;
        }

        fn clear_player_ui_selection_except_text(&mut self) {
            self.player_ui_selected_button_id = None;
            self.player_ui_selected_image_id = None;
            self.player_ui_selected_object_id = None;
        }

        fn clear_player_ui_selection_except_object(&mut self) {
            self.player_ui_selected_text_id = None;
            self.player_ui_selected_button_id = None;
            self.player_ui_selected_image_id = None;
            self.player_ui_text_editing_id = None;
        }

        fn find_box_by_id<'a>(
            boxes: &'a [PlayerUiTextBox],
            id: u32,
        ) -> Option<&'a PlayerUiTextBox> {
            boxes.iter().find(|b| b.id == id)
        }

        fn find_box_by_id_mut<'a>(
            boxes: &'a mut [PlayerUiTextBox],
            id: u32,
        ) -> Option<&'a mut PlayerUiTextBox> {
            boxes.iter_mut().find(|b| b.id == id)
        }

        fn player_ui_shift_active(&self) -> bool {
            self.shift_held || query_shift_held_os()
        }

        fn player_ui_ctrl_active(&self) -> bool {
            self.ctrl_held || query_ctrl_held_os()
        }

        fn hit_test_text_handle(
            &self,
            selected_id: u32,
            ndc: [f32; 2],
        ) -> Option<rer_engine_core::player_ui::config::PlayerUiResizeHandle> {
            let key = self.player_ui_text_key()?;
            let boxes = self.player_ui_text_boxes.get(&key)?;
            rer_engine_core::player_ui::text_input::hit_test::hit_test_text_handle(
                boxes,
                selected_id,
                ndc,
            )
        }

        fn hit_test_image_handle(
            &self,
            selected_id: u32,
            ndc: [f32; 2],
        ) -> Option<rer_engine_core::player_ui::config::PlayerUiResizeHandle> {
            let key = self.player_ui_text_key()?;
            let list = self.player_ui_images.get(&key)?;
            rer_engine_core::player_ui::text_input::hit_test::hit_test_image_handle(
                list,
                selected_id,
                ndc,
            )
        }

        fn hit_test_button_handle(
            &self,
            selected_id: u32,
            ndc: [f32; 2],
        ) -> Option<rer_engine_core::player_ui::config::PlayerUiResizeHandle> {
            let key = self.player_ui_text_key()?;
            let list = self.player_ui_buttons.get(&key)?;
            rer_engine_core::player_ui::text_input::hit_test::hit_test_button_handle(
                list,
                selected_id,
                ndc,
            )
        }

        fn hit_test_top_hud(
            &self,
            ndc: [f32; 2],
        ) -> Option<rer_engine_core::player_ui::text_input::types::PlayerUiHitTarget> {
            let key = self.player_ui_text_key()?;
            let texts = self
                .player_ui_text_boxes
                .get(&key)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let buttons = self
                .player_ui_buttons
                .get(&key)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let images = self
                .player_ui_images
                .get(&key)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let objects = self
                .player_ui_objects
                .get(&key)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            rer_engine_core::player_ui::text_input::hit_test::hit_test_top_hud(
                texts, buttons, images, objects, ndc,
            )
        }
    };
}
