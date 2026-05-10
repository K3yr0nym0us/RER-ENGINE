// ── Cámara 2D ortográfica — vista lateral (side-scroller / Hollow Knight) ────
//
// Coordenadas de mundo:  X = derecha, Y = arriba, Z = profundidad (sin usar).
// La cámara se sitúa a Z = +10 mirando hacia -Z.

use glam::{Mat4, Vec3};

pub(crate) struct Camera2D {
    /// Centro de la vista en X (seguimiento horizontal del personaje).
    pub x:      f32,
    /// Centro de la vista en Y (seguimiento vertical).
    pub y:      f32,
    /// Mitad de la altura visible en unidades de mundo.
    pub half_h: f32,
    pub near:   f32,
    pub far:    f32,
}

impl Camera2D {
    pub(crate) fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, 10.0)
    }

    /// Matriz view × proyección ortográfica lista para la GPU.
    ///
    /// La view matrix traslada el mundo a espacio de cámara restando (x, y),
    /// por lo que los límites de la ortográfica deben estar centrados en 0
    /// (espacio de vista). Usar [x±half_w, y±half_h] aplicaría el offset dos
    /// veces y desplazaría el renderizado respecto al picking.
    pub(crate) fn view_proj(&self, aspect: f32) -> Mat4 {
        let half_w = self.half_h * aspect;
        let proj = Mat4::orthographic_rh(
            -half_w, half_w,
            -self.half_h, self.half_h,
            self.near, self.far,
        );
        let view = Mat4::look_at_rh(
            Vec3::new(self.x, self.y, 10.0),
            Vec3::new(self.x, self.y,  0.0),
            Vec3::Y,
        );
        proj * view
    }

    /// Desplaza la cámara en el plano XY según el delta de píxeles del ratón.
    ///
    /// `dx` / `dy`   — delta en píxeles (positivo = derecha / abajo en pantalla).
    /// `vw` / `vh`   — tamaño del viewport en píxeles.
    pub(crate) fn pan(&mut self, dx: f32, dy: f32, vw: f32, vh: f32) {
        let aspect = vw / vh;
        let half_w = self.half_h * aspect;
        // Convertir delta de píxeles a unidades de mundo proporcionales al zoom
        self.x -= dx / vw * half_w  * 2.0;
        self.y += dy / vh * self.half_h * 2.0;  // Y de pantalla invertida respecto a mundo
    }
}
