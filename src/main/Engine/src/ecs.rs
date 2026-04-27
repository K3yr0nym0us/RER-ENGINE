// ---------------------------------------------------------------------------
// ECS — Entities, Components, Systems
//
// Arquitectura:
//   - Entity: solo un ID (u32)
//   - Component: cualquier tipo que implemente Component
//   - ComponentStorage<T>: Vec denso + mapa entidad→índice
//   - World: contiene todos los ComponentStorage registrados
// ---------------------------------------------------------------------------

use std::any::{Any, TypeId};
use std::collections::HashMap;
use glam::{Mat4, Quat, Vec3};

// ── Tipos base ────────────────────────────────────────────────────────────────
pub type EntityId = u32;

/// Genera un EntityId aleatorio via CSPRNG (no secuencial, no predecible).
/// Reintenta si colisiona con un ID ya existente (probabilidad astronómicamente baja).
fn new_entity_id(alive: &[EntityId]) -> EntityId {
    let mut buf = [0u8; 4];
    loop {
        getrandom::getrandom(&mut buf).expect("getrandom no disponible");
        let id = u32::from_ne_bytes(buf);
        // Evitar 0 (valor centinela) y colisiones
        if id != 0 && !alive.contains(&id) {
            return id;
        }
    }
}

// ── Componentes estándar ──────────────────────────────────────────────────────

/// Posición, rotación y escala de una entidad en el mundo.
#[derive(Debug, Clone)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale:    Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self { position: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }
}

impl Transform {
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

/// Referencia a la malla que renderiza esta entidad.
#[derive(Debug, Clone)]
pub struct MeshComponent {
    pub mesh_idx: usize,
}

/// Marca una entidad como no seleccionable por el usuario (escenario/fondo).
/// Las funciones de picking ignoran entidades con este componente.
#[derive(Debug, Clone, Default)]
pub struct NonSelectable;

/// Nombre visible en el SceneTree.
#[derive(Debug, Clone)]
pub struct NameComponent {
    pub name: String,
}

// ── Almacenamiento de componentes ─────────────────────────────────────────────

/// Almacenamiento denso para un tipo de componente.
/// Los accesos son O(1) a través del mapa entity→índice.
pub struct ComponentStorage<T> {
    data:       Vec<T>,
    entity_map: HashMap<EntityId, usize>,   // entity → índice en data
    index_map:  Vec<EntityId>,              // índice → entity (para iterar)
}

impl<T> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self { data: Vec::new(), entity_map: HashMap::new(), index_map: Vec::new() }
    }
}

impl<T> ComponentStorage<T> {
    pub fn insert(&mut self, entity: EntityId, component: T) {
        if let Some(&idx) = self.entity_map.get(&entity) {
            self.data[idx] = component;
        } else {
            let idx = self.data.len();
            self.data.push(component);
            self.entity_map.insert(entity, idx);
            self.index_map.push(entity);
        }
    }

    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.entity_map.get(&entity).map(|&i| &self.data[i])
    }

    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        self.entity_map.get(&entity).map(|&i| &mut self.data[i])
    }

    pub fn remove(&mut self, entity: EntityId) {
        if let Some(&idx) = self.entity_map.get(&entity) {
            let last = self.data.len() - 1;
            // Swap-remove para mantener densidad
            self.data.swap(idx, last);
            self.data.pop();
            let moved_entity = self.index_map[last];
            self.index_map.swap(idx, last);
            self.index_map.pop();
            self.entity_map.remove(&entity);
            if idx != last {
                self.entity_map.insert(moved_entity, idx);
            }
        }
    }

    /// Itera sobre todos los (EntityId, &T)
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.index_map.iter().copied().zip(self.data.iter())
    }

    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.index_map.iter().copied().zip(self.data.iter_mut())
    }
}

// ── Trait object para borrar tipo ─────────────────────────────────────────────

trait AnyStorage: Any {
    fn remove_entity(&mut self, entity: EntityId);
    fn as_any(&self)     -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: 'static> AnyStorage for ComponentStorage<T> {
    fn remove_entity(&mut self, entity: EntityId) { self.remove(entity); }
    fn as_any(&self)         -> &dyn Any     { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── World ─────────────────────────────────────────────────────────────────────

/// Mundo ECS: contiene todas las entidades y sus componentes.
pub struct World {
    alive:      Vec<EntityId>,
    storages:   HashMap<TypeId, Box<dyn AnyStorage>>,
}

impl Default for World {
    fn default() -> Self {
        let mut w = Self { alive: Vec::new(), storages: HashMap::new() };
        // Registrar almacenamientos estándar
        w.register::<Transform>();
        w.register::<MeshComponent>();
        w.register::<NameComponent>();
        w.register::<NonSelectable>();
        w
    }
}

impl World {
    pub fn new() -> Self { Self::default() }

    /// Registra un tipo de componente. Llamar antes de usar insert/get.
    pub fn register<T: 'static>(&mut self) {
        self.storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(ComponentStorage::<T>::default()));
    }

    // ── Entidades ─────────────────────────────────────────────────────────────

    /// Crea una nueva entidad con Transform por defecto y nombre opcional.
    pub fn spawn(&mut self, name: Option<&str>) -> EntityId {
        let id = new_entity_id(&self.alive);
        self.alive.push(id);
        self.insert(id, Transform::default());
        if let Some(n) = name {
            self.insert(id, NameComponent { name: n.to_owned() });
        }
        id
    }

    /// Destruye una entidad y elimina todos sus componentes.
    pub fn despawn(&mut self, entity: EntityId) {
        self.alive.retain(|&e| e != entity);
        for storage in self.storages.values_mut() {
            storage.remove_entity(entity);
        }
    }

    pub fn entities(&self) -> &[EntityId] { &self.alive }

    pub fn clear(&mut self) {
        let ids: Vec<_> = self.alive.clone();
        for id in ids { self.despawn(id); }
        self.alive.clear();
    }

    // ── Componentes ──────────────────────────────────────────────────────────

    pub fn insert<T: 'static>(&mut self, entity: EntityId, component: T) {
        self.register::<T>();
        let storage = self.storages
            .get_mut(&TypeId::of::<T>())
            .unwrap()
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .unwrap();
        storage.insert(entity, component);
    }

    pub fn get<T: 'static>(&self, entity: EntityId) -> Option<&T> {
        self.storages.get(&TypeId::of::<T>())?
            .as_any()
            .downcast_ref::<ComponentStorage<T>>()?
            .get(entity)
    }

    pub fn get_mut<T: 'static>(&mut self, entity: EntityId) -> Option<&mut T> {
        self.storages.get_mut(&TypeId::of::<T>())?
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()?
            .get_mut(entity)
    }

    #[allow(dead_code)]
    pub fn remove_component<T: 'static>(&mut self, entity: EntityId) {
        if let Some(s) = self.storages.get_mut(&TypeId::of::<T>()) {
            s.remove_entity(entity);
        }
    }

    /// Itera sobre todos los (EntityId, &T) de un tipo de componente.
    #[allow(dead_code)]
    pub fn query<T: 'static>(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<ComponentStorage<T>>())
            .map(|cs| cs.iter().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    #[allow(dead_code)]
    pub fn query_mut<T: 'static>(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| s.as_any_mut().downcast_mut::<ComponentStorage<T>>())
            .map(|cs| {
                // Safety: collect necesario porque el borrow de self.storages no puede durar
                let ptrs: Vec<(EntityId, *mut T)> = cs
                    .index_map.iter().copied()
                    .zip(cs.data.iter_mut().map(|c| c as *mut T))
                    .collect();
                ptrs
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(id, ptr)| (id, unsafe { &mut *ptr }))
    }

    /// Nombre de una entidad (None si no tiene NameComponent).
    pub fn name(&self, entity: EntityId) -> Option<&str> {
        self.get::<NameComponent>(entity).map(|n| n.name.as_str())
    }
}
