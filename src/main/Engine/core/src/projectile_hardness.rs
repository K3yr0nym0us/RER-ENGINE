//! Decisión de impacto proyectil↔entidad por tiers de dureza (2D y 3D).

/// Resultado de comparar dureza del proyectil con la de la entidad golpeada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardnessHitOutcome {
    /// Entidad con dureza 0: atravesar sin cambiar velocidad.
    Ignore,
    /// Atravesar aplicando (o no) pérdida de velocidad; no despawnea.
    Pierce {
        /// Multiplicador de `|v|` tras el hit (`1 - speed_loss`, o `1` si no hay pérdida).
        speed_retain: f32,
    },
    /// Rebote con reflect + pérdida de velocidad.
    Bounce { speed_retain: f32 },
    /// No puede rebotar (`bounces_left == 0`): hit final + despawn.
    Stop,
}

/// Resuelve el impacto según las reglas de dureza.
///
/// - `entity_hardness == 0` → Ignore
/// - `projectile_hardness == 0` → Pierce sin pérdida
/// - `projectile_hardness > entity_hardness` → Pierce con `bounce_speed_loss`
/// - `projectile_hardness <= entity_hardness` → Bounce si `bounces_left > 0`, si no Stop
pub fn resolve_hardness_hit(
    projectile_hardness: u32,
    entity_hardness: u32,
    bounces_left: u32,
    bounce_speed_loss: f32,
) -> HardnessHitOutcome {
    let loss = bounce_speed_loss.clamp(0.0, 1.0);
    let retain = (1.0 - loss).max(0.0);

    if entity_hardness == 0 {
        return HardnessHitOutcome::Ignore;
    }
    if projectile_hardness == 0 {
        return HardnessHitOutcome::Pierce { speed_retain: 1.0 };
    }
    if projectile_hardness > entity_hardness {
        return HardnessHitOutcome::Pierce {
            speed_retain: retain,
        };
    }
    if bounces_left > 0 {
        HardnessHitOutcome::Bounce {
            speed_retain: retain,
        }
    } else {
        HardnessHitOutcome::Stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_zero_ignores() {
        assert_eq!(
            resolve_hardness_hit(5, 0, 3, 0.2),
            HardnessHitOutcome::Ignore
        );
    }

    #[test]
    fn projectile_zero_pierces_without_loss() {
        assert_eq!(
            resolve_hardness_hit(0, 10, 0, 0.5),
            HardnessHitOutcome::Pierce { speed_retain: 1.0 }
        );
    }

    #[test]
    fn projectile_harder_pierces_with_loss() {
        assert_eq!(
            resolve_hardness_hit(5, 2, 3, 0.25),
            HardnessHitOutcome::Pierce { speed_retain: 0.75 }
        );
    }

    #[test]
    fn equal_or_softer_bounces_when_left() {
        assert_eq!(
            resolve_hardness_hit(2, 2, 1, 0.2),
            HardnessHitOutcome::Bounce { speed_retain: 0.8 }
        );
        assert_eq!(
            resolve_hardness_hit(1, 5, 2, 0.1),
            HardnessHitOutcome::Bounce { speed_retain: 0.9 }
        );
    }

    #[test]
    fn equal_or_softer_stops_without_bounces() {
        assert_eq!(resolve_hardness_hit(3, 3, 0, 0.2), HardnessHitOutcome::Stop);
    }
}
