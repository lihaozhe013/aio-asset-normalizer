use std::collections::HashMap;
use std::path::Path;
use three_d::*;

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub index: usize,
    pub parent_index: Option<usize>,
    pub children: Vec<usize>,
    pub local_rest_transform: Mat4,
    pub global_rest_transform: Mat4,
    pub inverse_bind_matrix: Mat4,
    pub node_index: usize,
}

impl Bone {
    pub fn position(&self) -> Vec3 {
        mat4_pos(&self.global_rest_transform)
    }

    pub fn parent_position(&self, skeleton: &Skeleton) -> Option<Vec3> {
        self.parent_index.map(|pi| skeleton.bones[pi].position())
    }
}

fn mat4_pos(m: &Mat4) -> Vec3 {
    vec3(m[3][0], m[3][1], m[3][2])
}

pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub root_index: Option<usize>,
    pub joint_to_bone: HashMap<usize, usize>,
    pub highlighted_bone: Option<usize>,
}

impl Skeleton {
    pub fn from_glb(path: &Path) -> Result<Self, String> {
        let (document, buffers, _images) =
            gltf::import(path).map_err(|e| format!("GLTF parse error: {}", e))?;

        let skin = document
            .skins()
            .next()
            .ok_or_else(|| "No skin found in GLB".to_string())?;

        let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));
        let ibms: Vec<[[f32; 4]; 4]> = reader
            .read_inverse_bind_matrices()
            .ok_or_else(|| "Missing inverse bind matrices".to_string())?
            .collect();

        let joints: Vec<gltf::Node> = skin.joints().collect();
        if joints.is_empty() {
            return Err("Skin has no joints".to_string());
        }

        let mut bone_nodes: HashMap<usize, usize> = HashMap::new();
        for (joint_idx, joint) in joints.iter().enumerate() {
            bone_nodes.insert(joint.index(), joint_idx);
        }

        let mut node_parent: HashMap<usize, usize> = HashMap::new();
        for node in document.nodes() {
            let parent_idx = node.index();
            for child in node.children() {
                node_parent.insert(child.index(), parent_idx);
            }
        }

        let mut bones: Vec<Bone> = Vec::with_capacity(joints.len());

        for (joint_idx, joint) in joints.iter().enumerate() {
            let node_idx = joint.index();
            let (t, r, s) = joint.transform().decomposed();
            let local = Mat4::from_translation(vec3(t[0], t[1], t[2]))
                * Mat4::from(Quat::new(r[3], r[0], r[1], r[2]))
                * Mat4::from_nonuniform_scale(s[0], s[1], s[2]);

            let ibm = ibms.get(joint_idx).copied().unwrap_or([[1.0; 4]; 4]);
            let ibm_mat = Mat4::new(
                ibm[0][0], ibm[1][0], ibm[2][0], ibm[3][0], ibm[0][1], ibm[1][1],
                ibm[2][1], ibm[3][1], ibm[0][2], ibm[1][2], ibm[2][2], ibm[3][2],
                ibm[0][3], ibm[1][3], ibm[2][3], ibm[3][3],
            );

            let parent_node_idx = node_parent.get(&node_idx).copied();
            let parent_index = parent_node_idx.and_then(|pni| bone_nodes.get(&pni).copied());

            bones.push(Bone {
                name: joint.name().unwrap_or("unnamed").to_string(),
                index: joint_idx,
                parent_index,
                children: Vec::new(),
                local_rest_transform: local,
                global_rest_transform: Mat4::identity(),
                inverse_bind_matrix: ibm_mat,
                node_index: node_idx,
            });
        }

        for i in 0..bones.len() {
            let children: Vec<usize> = joints[i]
                .children()
                .filter_map(|c| bone_nodes.get(&c.index()).copied())
                .collect();
            bones[i].children = children;
        }

        let root_index = bones.iter().position(|b| b.parent_index.is_none());
        if let Some(ri) = root_index {
            let root_global = bones[ri].local_rest_transform;
            bones[ri].global_rest_transform = root_global;
            let children: Vec<usize> = bones[ri].children.clone();
            for child in &children {
                propagate_global_transform(&mut bones, *child, root_global);
            }
        } else if let Some(first) = bones.first_mut() {
            first.global_rest_transform = first.local_rest_transform;
        }

        let joint_to_bone: HashMap<usize, usize> =
            bone_nodes.into_iter().map(|(k, v)| (k, v)).collect();

        Ok(Self {
            bones,
            root_index,
            joint_to_bone,
            highlighted_bone: None,
        })
    }

    pub fn bone_positions(&self) -> Vec<(Vec3, Vec3)> {
        let mut segments = Vec::new();
        for bone in &self.bones {
            if let Some(parent_pos) = bone.parent_position(self) {
                segments.push((parent_pos, bone.position()));
            }
        }
        segments
    }

    pub fn joint_positions(&self) -> Vec<Vec3> {
        self.bones.iter().map(|b| b.position()).collect()
    }
}

fn propagate_global_transform(bones: &mut [Bone], idx: usize, parent_global: Mat4) {
    let local = bones[idx].local_rest_transform;
    let new_global = parent_global * local;
    bones[idx].global_rest_transform = new_global;
    let children: Vec<usize> = bones[idx].children.clone();
    for child in children {
        propagate_global_transform(bones, child, new_global);
    }
}
