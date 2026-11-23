use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct EntitySnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub facing_right: bool,
    pub facing_sign: f32,
    /// Nombres de clip. `Arc` para no clonar las strings cada tick de play.
    pub animations: Arc<[String]>,
}

/// Reusa los nombres de clip si el conjunto no cambió (mismo count y mismas keys).
pub fn cached_clip_names<V>(
    cache: &mut HashMap<u32, Arc<[String]>>,
    id: u32,
    clips: Option<&HashMap<String, V>>,
) -> Arc<[String]> {
    let Some(map) = clips else {
        cache.remove(&id);
        return Arc::from([]);
    };
    if let Some(names) = cache.get(&id)
        && names.len() == map.len()
        && names.iter().all(|name| map.contains_key(name))
    {
        return Arc::clone(names);
    }
    let names: Arc<[String]> = map.keys().cloned().collect();
    cache.insert(id, Arc::clone(&names));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_clip_names_reuses_arc_when_keys_match() {
        let mut cache = HashMap::new();
        let mut clips = HashMap::new();
        clips.insert("idle".to_string(), ());
        clips.insert("walk".to_string(), ());
        let first = cached_clip_names(&mut cache, 7, Some(&clips));
        let second = cached_clip_names(&mut cache, 7, Some(&clips));
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.len(), 2);
        clips.insert("run".to_string(), ());
        let third = cached_clip_names(&mut cache, 7, Some(&clips));
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(third.len(), 3);
    }
}
