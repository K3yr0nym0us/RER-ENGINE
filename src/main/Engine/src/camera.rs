// Este archivo fue dividido en:
//   CONFIG_3D/camera_3d.rs  — Camera (órbita 3D) + CameraUniform
//   CONFIG_2D/camera_2d.rs  — Camera2D (ortográfica 2D)
//
// Ya no se incluye como módulo en main.rs.


// ---------------------------------------------------------------------------
// Datos que se suben a la GPU (debe ser Pod + repr(C))
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

// ---------------------------------------------------------------------------
// Cámara en CPU — modo órbita alrededor de un target
// ---------------------------------------------------------------------------
pub struct Camera {
    /// Punto alrededor del que orbita la cámara.
    pub target:   Vec3,
    /// Distancia al target.
    pub distance: f32,
    /// Ángulo horizontal (azimut) en radianes.
    pub yaw:      f32,
    /// Ángulo vertical (elevación) en radianes, clampeado para no girar 360.
    pub pitch:    f32,
    pub fov_y:    f32,
    pub near:     f32,
    pub far:      f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            target:   Vec3::ZERO,
            distance: 3.0,
            yaw:      -std::f32::consts::FRAC_PI_4,
            pitch:    0.3,
            fov_y:    45_f32.to_radians(),
            near:     0.1,
            far:      1000.0,
        }
    }

    /// Posición calculada desde órbita.
    pub fn position(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cy * cp, sp, sy * cp) * self.distance
    }

    /// Orbitar: delta en píxeles → cambio de yaw/pitch.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.005;
        self.yaw   += dx * SENSITIVITY;
        self.pitch  = (self.pitch - dy * SENSITIVITY)
            .clamp(-std::f32::consts::FRAC_PI_2 + 0.05, std::f32::consts::FRAC_PI_2 - 0.05);
    }

    /// Zoom: delta positivo = acercar.
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta * 0.3).clamp(0.5, 500.0);
    }

    /// Pan: desplazar el target en el plano de la cámara.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.002;
        let pos    = self.position();
        let fwd    = (self.target - pos).normalize();
        let right  = fwd.cross(Vec3::Y).normalize();
        let up     = right.cross(fwd).normalize();
        let offset = right * (-dx * SENSITIVITY * self.distance)
                   + up   * ( dy * SENSITIVITY * self.distance);
        self.target += offset;
    }

    pub fn view_matrix(&self) -> Mat4 {
        let pos = self.position();
        Mat4::look_at_rh(pos, self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far)
    }

    pub fn to_uniform(&self, aspect: f32) -> CameraUniform {
        CameraUniform {
            view_proj: (self.proj_matrix(aspect) * self.view_matrix()).to_cols_array_2d(),
        }
    }
}

// ---------------------------------------------------------------------------
// Cámara 2D ortográfica — vista lateral (side-scroller / Hollow Knight)
//
// Coordenadas de mundo:  X = derecha, Y = arriba, Z = profundidad (sin usar)
// La cámara se sitúa a Z = +10 mirando hacia -Z.
// ---------------------------------------------------------------------------
pub struct Camera2D {
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
    pub fn new() -> Self {
        Self {
            x:      0.0,
            y:      4.0,    // centrada a 4 u por encima del suelo
            half_h: 6.0,    // 12 u de alto visibles
            near:   -100.0,
            far:    100.0,
        }
    }

    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, 10.0)
    }

    /// Matriz view × proyección ortográfica lista para la GPU.
    ///
    /// La view matrix ya traslada el mundo a espacio de cámara restando (x, y),
    /// por lo que los límites de la ortográfica deben estar centrados en 0
    /// (espacio de vista). Usar [x±half_w, y±half_h] aplicaría el offset dos
    /// veces y desplazaría el renderizado respecto al picking.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
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
}
