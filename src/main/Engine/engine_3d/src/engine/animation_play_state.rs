//! Estado de reproducción de animaciones (clips 3D y sprites) para el editor.

use crate::ecs::EntityId;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
  pub(crate) fn entity_animation_play_state(
    &self,
    entity_id: EntityId,
  ) -> (bool, Option<String>, Option<bool>) {
    if let Some(active) = self.active_model_clips.get(&entity_id) {
      if active.playing && !active.finished {
        return (
          true,
          Some(active.clip_name.clone()),
          Some(active.loop_),
        );
      }
    }

    if let Some(active) = self.active_animations.get(&entity_id) {
      if !active.finished {
        let loop_ = self
          .animations
          .get(&entity_id)
          .and_then(|m| m.get(&active.animation_name))
          .map(|a| a.loop_);
        return (
          true,
          Some(active.animation_name.clone()),
          loop_,
        );
      }
    }

    (false, None, None)
  }

  pub(crate) fn emit_entity_animation_play_state(&self, entity_id: EntityId) {
    let (playing, name, loop_) = self.entity_animation_play_state(entity_id);
    send_event(&EngineEvent::EntityAnimationPlayState {
      entity_id,
      playing,
      name,
      loop_,
    });
  }
}
