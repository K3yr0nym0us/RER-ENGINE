//! Lectura y volcado de keyframes de animación glTF.

use super::*;

pub(crate) fn push_gltf_keyframe(
    keyframes: &mut Vec<AnimKeyframe>,
    property: &AnimProperty,
    t: f32,
    out: [f32; 4],
) {
    let kf = match *property {
        AnimProperty::Translation => AnimKeyframe {
            time: t,
            translation: Some([out[0], out[1], out[2]]),
            rotation: None,
            scale: None,
        },
        AnimProperty::Scale => AnimKeyframe {
            time: t,
            translation: None,
            rotation: None,
            scale: Some([out[0], out[1], out[2]]),
        },
        AnimProperty::Rotation => AnimKeyframe {
            time: t,
            translation: None,
            rotation: Some([out[0], out[1], out[2], out[3]]),
            scale: None,
        },
    };
    keyframes.push(kf);
}

pub(crate) fn parse_gltf_animation(
    anim: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    node_to_joint: &HashMap<usize, usize>,
) -> Option<AnimationClip> {
    let name = anim
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Animation_{}", anim.index()));

    let mut channels = Vec::new();
    let mut max_time = 0.0f32;

    for channel in anim.channels() {
        let target_node = channel.target().node().index();
        let joint_index = match node_to_joint.get(&target_node) {
            Some(&ji) => ji,
            None => continue,
        };
        let property = match channel.target().property() {
            gltf::animation::Property::Translation => AnimProperty::Translation,
            gltf::animation::Property::Rotation => AnimProperty::Rotation,
            gltf::animation::Property::Scale => AnimProperty::Scale,
            _ => continue,
        };
        let reader = channel.reader(|b| Some(&buffers[b.index()].0[..]));
        let Some(inputs) = reader.read_inputs() else {
            continue;
        };
        let inputs: Vec<f32> = inputs.collect();
        const MAX_GLTF_KEYS_PER_CHANNEL: usize = 4096;
        let outputs: Vec<[f32; 4]> = match reader.read_outputs() {
            Some(gltf::animation::util::ReadOutputs::Translations(iter))
                if matches!(property, AnimProperty::Translation) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Scales(iter))
                if matches!(property, AnimProperty::Scale) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Rotations(
                gltf::animation::util::Rotations::F32(iter),
            )) if matches!(property, AnimProperty::Rotation) => {
                iter.map(|q| [q[0], q[1], q[2], q[3]]).collect()
            }
            _ => continue,
        };

        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }

        let mut keyframes = Vec::new();
        let key_count = inputs.len().min(outputs.len());
        if key_count > MAX_GLTF_KEYS_PER_CHANNEL {
            let step = key_count as f32 / MAX_GLTF_KEYS_PER_CHANNEL as f32;
            for k in 0..MAX_GLTF_KEYS_PER_CHANNEL {
                let i = ((k as f32 * step) as usize).min(key_count - 1);
                let t = inputs[i];
                let out = outputs[i];
                max_time = max_time.max(t);
                push_gltf_keyframe(&mut keyframes, &property, t, out);
            }
        } else {
            for (t, out) in inputs.iter().zip(outputs.iter()) {
                max_time = max_time.max(*t);
                push_gltf_keyframe(&mut keyframes, &property, *t, *out);
            }
        }
        keyframes.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        channels.push(AnimChannel {
            joint_index,
            property,
            keyframes,
        });
    }

    let duration_s = max_time.max(1.0 / 30.0);
    let fps = 30.0;

    Some(AnimationClip {
        name,
        duration_s,
        fps,
        channels,
    })
}
