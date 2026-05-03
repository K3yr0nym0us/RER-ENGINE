// ── CONFIG_SHARED — utilidades compartidas entre todos los modos ──────────────
//
// Contiene funciones usadas por más de un modo de escena (2D, 3D, BASE):
//  · point_to_segment_2d   — distancia de un punto a un segmento 2D (picking)

/// Distancia 2D desde el punto `(px, py)` al segmento `[(ax,ay), (bx,by)]`.
/// Usada por el picking de ejes de gizmo tanto en modo 2D como 3D.
pub(crate) fn point_to_segment_2d(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t  = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t  = t.clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}
