use glam::{Mat4, Quat, Vec3};

pub type EntityId = u32;

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale:    Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale:    Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

// ---------------------------------------------------------------------------
// Entity
// ---------------------------------------------------------------------------
#[derive(Debug)]
pub struct Entity {
    pub id:        EntityId,
    pub transform: Transform,
    /// Índice en el Vec<Mesh> del engine (-1 = sin mesh)
    pub mesh_idx:  Option<usize>,
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------
#[derive(Default)]
pub struct Scene {
    next_id:  EntityId,
    entities: Vec<Entity>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_entity(&mut self, mesh_idx: Option<usize>) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.push(Entity {
            id,
            transform: Transform::default(),
            mesh_idx,
        });
        id
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn remove(&mut self, id: EntityId) {
        self.entities.retain(|e| e.id != id);
    }

    pub fn clear(&mut self) {
        self.entities.clear();
    }
}
