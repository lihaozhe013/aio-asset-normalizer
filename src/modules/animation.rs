use std::path::Path;
use three_d::*;

use super::skeleton::Skeleton;

#[derive(Debug, Clone)]
pub struct Keyframe<T: Clone> {
    pub time: f32,
    pub value: T,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub bone_index: usize,
    pub translation_keys: Vec<Keyframe<Vec3>>,
    pub rotation_keys: Vec<Keyframe<Quat>>,
    pub scale_keys: Vec<Keyframe<Vec3>>,
}

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<Channel>,
}

pub struct AnimationPlayer {
    pub clips: Vec<AnimationClip>,
    pub current_clip: usize,
    pub current_time: f32,
    pub playing: bool,
    pub looping: bool,
    pub speed: f32,
    pub bone_transforms: Vec<Mat4>,
}

impl AnimationPlayer {
    pub fn from_glb(path: &Path, skeleton: &Skeleton) -> Result<Self, String> {
        let (document, buffers, _images) =
            gltf::import(path).map_err(|e| format!("GLTF parse error: {}", e))?;

        let get_buf = |buffer: gltf::Buffer| Some(&buffers[buffer.index()]);

        let mut clips = Vec::new();

        for anim in document.animations() {
            let name = anim.name().unwrap_or("Unnamed").to_string();
            let mut channels: Vec<Channel> = Vec::new();
            let mut max_time = 0.0f32;

            for channel in anim.channels() {
                let target = channel.target();
                let node_idx = target.node().index();

                let bone_idx = match skeleton.joint_to_bone.get(&node_idx) {
                    Some(&bi) => bi,
                    None => continue,
                };

                let sampler = channel.sampler();
                let input_accessor = sampler.input();
                let output_accessor = sampler.output();

                let buf_fn = |buf: gltf::Buffer| get_buf(buf).map(|d| &**d);

                let times: Vec<f32> = match gltf::accessor::Iter::new(input_accessor, buf_fn) {
                    Some(iter) => iter.collect(),
                    None => continue,
                };

                if let Some(&t) = times.last() {
                    max_time = max_time.max(t);
                }

                let property = target.property();
                let mut ch = Channel {
                    bone_index: bone_idx,
                    translation_keys: Vec::new(),
                    rotation_keys: Vec::new(),
                    scale_keys: Vec::new(),
                };

                let has_data = match property {
                    gltf::animation::Property::Translation => {
                        if let Some(iter) =
                            gltf::accessor::Iter::<[f32; 3]>::new(output_accessor, buf_fn)
                        {
                            let values: Vec<Vec3> = iter.map(|v| vec3(v[0], v[1], v[2])).collect();
                            ch.translation_keys = times
                                .into_iter()
                                .zip(values)
                                .map(|(t, v)| Keyframe { time: t, value: v })
                                .collect();
                            true
                        } else {
                            false
                        }
                    }
                    gltf::animation::Property::Rotation => {
                        if let Some(iter) =
                            gltf::accessor::Iter::<[f32; 4]>::new(output_accessor, buf_fn)
                        {
                            let values: Vec<Quat> =
                                iter.map(|v| Quat::new(v[3], v[0], v[1], v[2])).collect();
                            ch.rotation_keys = times
                                .into_iter()
                                .zip(values)
                                .map(|(t, v)| Keyframe { time: t, value: v })
                                .collect();
                            true
                        } else {
                            false
                        }
                    }
                    gltf::animation::Property::Scale => {
                        if let Some(iter) =
                            gltf::accessor::Iter::<[f32; 3]>::new(output_accessor, buf_fn)
                        {
                            let values: Vec<Vec3> = iter.map(|v| vec3(v[0], v[1], v[2])).collect();
                            ch.scale_keys = times
                                .into_iter()
                                .zip(values)
                                .map(|(t, v)| Keyframe { time: t, value: v })
                                .collect();
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                if has_data {
                    channels.push(ch);
                }
            }

            if !channels.is_empty() {
                clips.push(AnimationClip {
                    name,
                    duration: max_time,
                    channels,
                });
            }
        }

        if clips.is_empty() {
            return Err("No animation clips found".to_string());
        }

        let bone_count = skeleton.bones.len();
        Ok(Self {
            clips,
            current_clip: 0,
            current_time: 0.0,
            playing: false,
            looping: false,
            speed: 1.0,
            bone_transforms: vec![Mat4::identity(); bone_count],
        })
    }

    pub fn current_clip(&self) -> Option<&AnimationClip> {
        self.clips.get(self.current_clip)
    }

    pub fn duration(&self) -> f32 {
        self.clips
            .get(self.current_clip)
            .map(|c| c.duration)
            .unwrap_or(0.0)
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        let duration = self.duration();
        self.current_time += dt * self.speed;
        if self.current_time >= duration && duration > 0.0 {
            if self.looping {
                self.current_time %= duration;
            } else {
                self.current_time = duration;
                self.playing = false;
            }
        }
        self.update_bone_transforms();
    }

    pub fn update_bone_transforms(&mut self) {
        let bone_count = self.bone_transforms.len();
        self.bone_transforms = vec![Mat4::identity(); bone_count];

        let channels = match self.clips.get(self.current_clip) {
            Some(c) => c.channels.clone(),
            None => return,
        };

        let t = self.current_time;

        for channel in &channels {
            if channel.bone_index >= bone_count {
                continue;
            }

            let translation = if !channel.translation_keys.is_empty() {
                interpolate_vec3(t, &channel.translation_keys)
            } else {
                None
            };

            let rotation = if !channel.rotation_keys.is_empty() {
                interpolate_quat(t, &channel.rotation_keys)
            } else {
                None
            };

            let scale = if !channel.scale_keys.is_empty() {
                interpolate_vec3(t, &channel.scale_keys)
            } else {
                None
            };

            let mut local = Mat4::identity();
            if let Some(tr) = translation {
                local = local * Mat4::from_translation(tr);
            }
            if let Some(rt) = rotation {
                local = local * Mat4::from(rt);
            }
            if let Some(sc) = scale {
                local = local * Mat4::from_nonuniform_scale(sc.x, sc.y, sc.z);
            }

            self.bone_transforms[channel.bone_index] = local;
        }
    }

    pub fn animated_bone_positions(&self, skeleton: &Skeleton) -> (Vec<(Vec3, Vec3)>, Vec<Vec3>) {
        let mut global_transforms: Vec<Mat4> = vec![Mat4::identity(); skeleton.bones.len()];

        for bone in &skeleton.bones {
            let local = self
                .bone_transforms
                .get(bone.index)
                .copied()
                .unwrap_or(bone.local_rest_transform);
            let parent_global = bone
                .parent_index
                .and_then(|pi| global_transforms.get(pi))
                .copied()
                .unwrap_or(Mat4::identity());
            global_transforms[bone.index] = parent_global * local;
        }

        let mut segments = Vec::new();
        let mut joints = Vec::new();

        for bone in &skeleton.bones {
            let m = &global_transforms[bone.index];
            let pos = vec3(m[3][0], m[3][1], m[3][2]);
            joints.push(pos);

            if let Some(parent_idx) = bone.parent_index {
                let pm = &global_transforms[parent_idx];
                let parent_pos = vec3(pm[3][0], pm[3][1], pm[3][2]);
                segments.push((parent_pos, pos));
            }
        }

        (segments, joints)
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.current_time = 0.0;
        self.update_bone_transforms();
    }

    pub fn toggle_play(&mut self) {
        if self.playing {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn set_clip(&mut self, index: usize) {
        if index < self.clips.len() {
            self.current_clip = index;
            self.current_time = 0.0;
            self.update_bone_transforms();
        }
    }

    pub fn clip_names(&self) -> Vec<String> {
        self.clips.iter().map(|c| c.name.clone()).collect()
    }
}

fn interpolate_vec3(t: f32, keys: &[Keyframe<Vec3>]) -> Option<Vec3> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || t <= keys[0].time {
        return Some(keys[0].value);
    }
    if t >= keys.last().unwrap().time {
        return Some(keys.last().unwrap().value);
    }
    for i in 0..keys.len() - 1 {
        if t >= keys[i].time && t <= keys[i + 1].time {
            let dt = keys[i + 1].time - keys[i].time;
            let factor = if dt > 0.0 {
                (t - keys[i].time) / dt
            } else {
                0.0
            };
            return Some(keys[i].value + (keys[i + 1].value - keys[i].value) * factor);
        }
    }
    Some(keys[0].value)
}

fn interpolate_quat(t: f32, keys: &[Keyframe<Quat>]) -> Option<Quat> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || t <= keys[0].time {
        return Some(keys[0].value);
    }
    if t >= keys.last().unwrap().time {
        return Some(keys.last().unwrap().value);
    }
    for i in 0..keys.len() - 1 {
        if t >= keys[i].time && t <= keys[i + 1].time {
            let dt = keys[i + 1].time - keys[i].time;
            let factor = if dt > 0.0 {
                (t - keys[i].time) / dt
            } else {
                0.0
            };
            return Some(keys[i].value.slerp(keys[i + 1].value, factor));
        }
    }
    Some(keys[0].value)
}
