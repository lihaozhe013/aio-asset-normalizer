use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

use crate::modules::glb::SkinNode;

use super::*;

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn markdown_fence_handles_asset_backticks() {
    let source = SkeletonDescriptor {
        kind: SourceKind::Bvh,
        file_sha256: "file".to_owned(),
        skeleton_sha256: "skeleton".to_owned(),
        skin: None,
        root: 0,
        up_axis: "Y".to_owned(),
        forward_axis: "-Z".to_owned(),
        unit: "m".to_owned(),
        nodes: vec![SkeletonNode {
            index: 0,
            name: "```asset```".to_owned(),
            path: vec!["```asset```".to_owned()],
            parent: None,
            children: Vec::new(),
            translation: [0.0; 3],
            rotation: identity_quaternion(),
            scale: [1.0; 3],
            is_skin_joint: true,
            animated: true,
        }],
        animated_nodes: vec![0],
        mesh_nodes: Vec::new(),
    };
    let target = source.clone();
    let prompt = build_agent_prompt(&source, &target, None, None).unwrap();
    assert!(prompt.contains("```asset```"));
    assert!(!prompt.contains("C:\\Users\\"));
}

#[test]
fn mapping_round_trips_as_versioned_json() {
    let endpoint = MappingEndpoint {
        kind: SourceKind::Bvh,
        file_sha256: String::new(),
        skeleton_sha256: String::new(),
        skin: None,
        root: NodeRef::new("Root", vec!["Root".to_owned()], 0),
        up_axis: "Y".to_owned(),
        forward_axis: "-Z".to_owned(),
        unit: "m".to_owned(),
    };
    let mut target = endpoint.clone();
    target.kind = SourceKind::Glb;
    target.skin = Some(SkinRef {
        index: 0,
        name: "Skin".to_owned(),
    });
    let mapping = SkeletonMapping::new(endpoint, target);
    let value = serde_json::to_value(&mapping).unwrap();
    assert_eq!(value["schema"], MAPPING_SCHEMA);
    assert_eq!(value["version"], MAPPING_VERSION);
}

#[test]
fn legacy_name_lookup_rejects_names_repeated_more_than_twice() {
    let descriptor = SkeletonDescriptor {
        kind: SourceKind::Bvh,
        file_sha256: String::new(),
        skeleton_sha256: String::new(),
        skin: None,
        root: 0,
        up_axis: "Y".to_owned(),
        forward_axis: "-Z".to_owned(),
        unit: "m".to_owned(),
        nodes: (0..3)
            .map(|index| SkeletonNode {
                index,
                name: "Root".to_owned(),
                path: vec![format!("Root{index}")],
                parent: None,
                children: Vec::new(),
                translation: [0.0; 3],
                rotation: identity_quaternion(),
                scale: [1.0; 3],
                is_skin_joint: true,
                animated: true,
            })
            .collect(),
        animated_nodes: vec![0, 1, 2],
        mesh_nodes: Vec::new(),
    };
    assert!(unique_name_lookup(&descriptor).get("Root").is_none());
}

#[test]
fn bvh_rest_pose_is_offsets_not_the_first_motion_frame() {
    let document = BvhDocument::parse(
            "HIERARCHY\nROOT Root\n{\nOFFSET 0 0 0\nCHANNELS 3 Zrotation Xrotation Yrotation\nJOINT Child\n{\nOFFSET 0 1 0\nCHANNELS 3 Zrotation Xrotation Yrotation\n}\n}\nMOTION\nFrames: 2\nFrame Time: 0.1\n30 0 0 10 0 0\n60 0 0 20 0 0\n",
        )
        .unwrap();
    let target = SkinData {
        index: 0,
        name: "TargetSkin".to_owned(),
        skeleton: None,
        joints: vec![0, 1],
        mesh_nodes: Vec::new(),
        nodes: vec![
            crate::modules::glb::SkinNode {
                index: 0,
                name: "Root".to_owned(),
                parent: None,
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
            crate::modules::glb::SkinNode {
                index: 1,
                name: "Child".to_owned(),
                parent: Some(0),
                translation: [0.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
        ],
    };
    let source_descriptor =
        SkeletonDescriptor::from_bvh(&document, String::new(), "Y", "-Z", "m")
            .unwrap();
    let target_descriptor = SkeletonDescriptor::from_skin(
        &target,
        SourceKind::Glb,
        String::new(),
        String::new(),
        "Y",
        "-Z",
        "m",
        &HashSet::new(),
    )
    .unwrap();
    let mut mapping = SkeletonMapping::new(
        source_descriptor.endpoint(),
        target_descriptor.endpoint(),
    );
    mapping.bones = vec![
        MappingBone {
            source: source_descriptor.node_ref(0),
            target: target_descriptor.node_ref(0),
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MappingBone {
            source: source_descriptor.node_ref(1),
            target: target_descriptor.node_ref(1),
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    mapping.root_motion = Some(RootMotionMapping {
        source: source_descriptor.node_ref(0),
        target: target_descriptor.node_ref(0),
        translation_scale: 1.0,
    });
    let clip = retarget_bvh(
        &document,
        &target,
        &mapping,
        RetargetOptions::default(),
        "Test",
    )
    .unwrap();
    let root = &clip.channels[0].rotations;
    let child = &clip.channels[1].rotations;
    let first_root_angle = 2.0 * root[0][2].atan2(root[0][3]).to_degrees();
    let first_child_angle = 2.0 * child[0][2].atan2(child[0][3]).to_degrees();
    assert!((first_root_angle - 30.0).abs() < 1.0e-3);
    assert!((first_child_angle - 10.0).abs() < 1.0e-3);
    let root_angle = 2.0 * root[1][2].atan2(root[1][3]).to_degrees();
    let child_angle = 2.0 * child[1][2].atan2(child[1][3]).to_degrees();
    assert!((root_angle - 60.0).abs() < 1.0e-3);
    assert!((child_angle - 20.0).abs() < 1.0e-3);
}

#[test]
fn glb_to_glb_retarget_replaces_only_target_animations() {
    let source_path = write_glb_fixture(
        "glb-retarget-source",
        ["SourceRoot", "SourceChild"],
        true,
        false,
    );
    let target_path = write_glb_fixture(
        "glb-retarget-target",
        ["TargetRoot", "TargetChild"],
        false,
        true,
    );
    let source_document =
        crate::modules::glb::GlbDocument::load(&source_path).unwrap();
    let target_document =
        crate::modules::glb::GlbDocument::load(&target_path).unwrap();
    let source_bytes = source_document.to_bytes().unwrap();
    let runtime = AnimationRuntime::from_bytes_skeleton_only(
        &source_bytes,
        source_path.parent(),
    )
    .unwrap();
    let animated_nodes = runtime.clips[0]
        .channels
        .iter()
        .map(|channel| channel.node)
        .collect::<HashSet<_>>();
    let source = SkeletonDescriptor::from_runtime(
        &runtime,
        &source_document,
        0,
        &animated_nodes,
        sha256_hex(&source_bytes),
        "Y".to_owned(),
        "-Z".to_owned(),
        "m".to_owned(),
    )
    .unwrap();
    let target_skin = target_document.skin_data_at(0).unwrap();
    let target = SkeletonDescriptor::from_skin(
        &target_skin,
        SourceKind::Glb,
        sha256_hex(&target_document.to_bytes().unwrap()),
        String::new(),
        "Y",
        "-Z",
        "m",
        &HashSet::new(),
    )
    .unwrap();
    let mut mapping =
        SkeletonMapping::new(source.endpoint(), target.endpoint());
    mapping.bones = vec![
        MappingBone {
            source: source.node_ref(0),
            target: target.node_ref(0),
            rotation_offset_xyzw: identity_quaternion(),
        },
        MappingBone {
            source: source.node_ref(1),
            target: target.node_ref(1),
            rotation_offset_xyzw: identity_quaternion(),
        },
    ];
    mapping.root_motion = Some(RootMotionMapping {
        source: source.node_ref(0),
        target: target.node_ref(0),
        translation_scale: 1.0,
    });

    let clip = retarget_glb(
        &runtime,
        &source_document,
        0,
        &target_skin,
        &mapping,
        RetargetOptions::default(),
        "Retargeted",
    )
    .unwrap();
    assert_eq!(clip.times.first().copied(), Some(0.0));
    assert_eq!(clip.times.last().copied(), Some(1.0));
    assert!(clip.times.len() >= 61);
    let mut output = target_document.clone();
    output
        .replace_animations(&crate::modules::glb::AnimationClipData {
            name: clip.name,
            times: clip.times,
            channels: clip.channels,
        })
        .unwrap();
    let output_bytes = output.to_bytes().unwrap();
    gltf::Gltf::from_slice(&output_bytes).unwrap();
    let glb = gltf::binary::Glb::from_slice(&output_bytes).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&glb.json).unwrap();
    assert_eq!(json["animations"].as_array().unwrap().len(), 1);
    assert_eq!(json["animations"][0]["name"], "Retargeted");
    assert_eq!(json["extras"]["keep"], true);
    assert_eq!(json["extensions"]["EXT_fixture"]["keep"], true);

    let output_runtime = AnimationRuntime::from_bytes_skeleton_only(
        &output_bytes,
        target_path.parent(),
    )
    .unwrap();
    let pose = output_runtime.sample_nodes(0, 1.0).unwrap();
    let angle = 2.0
        * pose[0].world_rotation[2]
            .atan2(pose[0].world_rotation[3])
            .to_degrees();
    assert!((angle - 90.0).abs() < 1.0e-2);

    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(target_path);
}

#[test]
fn bvh_preview_positions_use_explicit_axes_and_units() {
    let converted =
        convert_bvh_position_to_glb([100.0, 0.0, 0.0], "Y", "-Z", "cm")
            .unwrap();
    assert_eq!(converted, [1.0, 0.0, 0.0]);

    let converted =
        convert_bvh_position_to_glb([0.0, 0.0, 100.0], "Y", "+Z", "cm")
            .unwrap();
    assert_eq!(converted, [0.0, 0.0, -1.0]);
}

#[test]
fn root_motion_is_rebased_and_scaled_or_can_be_disabled() {
    let document = BvhDocument::parse(
            "HIERARCHY\nROOT Root\n{\nOFFSET 0 0 0\nCHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\n}\nMOTION\nFrames: 2\nFrame Time: 0.1\n10 0 0 0 0 0\n30 0 0 0 0 0\n",
        )
        .unwrap();
    let target = SkinData {
        index: 0,
        name: "TargetSkin".to_owned(),
        skeleton: None,
        joints: vec![0],
        mesh_nodes: Vec::new(),
        nodes: vec![SkinNode {
            index: 0,
            name: "Root".to_owned(),
            parent: None,
            translation: [0.0, 0.0, 0.0],
            rotation: identity_quaternion(),
            scale: [1.0; 3],
        }],
    };
    let source =
        SkeletonDescriptor::from_bvh(&document, String::new(), "Y", "-Z", "m")
            .unwrap();
    let target_descriptor = SkeletonDescriptor::from_skin(
        &target,
        SourceKind::Glb,
        String::new(),
        String::new(),
        "Y",
        "-Z",
        "m",
        &HashSet::new(),
    )
    .unwrap();
    let mut mapping =
        SkeletonMapping::new(source.endpoint(), target_descriptor.endpoint());
    mapping.bones = vec![MappingBone {
        source: source.node_ref(0),
        target: target_descriptor.node_ref(0),
        rotation_offset_xyzw: identity_quaternion(),
    }];
    mapping.root_motion = Some(RootMotionMapping {
        source: source.node_ref(0),
        target: target_descriptor.node_ref(0),
        translation_scale: 2.0,
    });
    let clip = retarget_bvh(
        &document,
        &target,
        &mapping,
        RetargetOptions::default(),
        "Root motion",
    )
    .unwrap();
    let translations = clip.channels[0].translations.as_ref().unwrap();
    assert_eq!(translations[0], [0.0, 0.0, 0.0]);
    assert_eq!(translations[1], [40.0, 0.0, 0.0]);

    let clip = retarget_bvh(
        &document,
        &target,
        &mapping,
        RetargetOptions {
            root_motion: false,
            ..RetargetOptions::default()
        },
        "No root motion",
    )
    .unwrap();
    assert!(clip.channels[0].translations.is_none());
}

fn write_glb_fixture(
    prefix: &str,
    names: [&str; 2],
    animated: bool,
    target_metadata: bool,
) -> PathBuf {
    let mut bin = Vec::new();
    for value in [0.0_f32, 1.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for rotation in
        [[0.0_f32, 0.0, 0.0, 1.0], [0.0, 0.0, 0.70710677, 0.70710677]]
    {
        for value in rotation {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut document = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": names[0], "children": [1]},
            {"name": names[1]}
        ],
        "skins": [{"name": "FixtureSkin", "joints": [0, 1], "skeleton": 0}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 8},
            {"buffer": 0, "byteOffset": 8, "byteLength": 32}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0]},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"}
        ]
    });
    if animated || target_metadata {
        document["animations"] = json!([{
            "name": if animated { "Source" } else { "Original" },
            "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}],
            "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}]
        }]);
    }
    if target_metadata {
        document["extras"] = json!({"keep": true});
        document["extensions"] = json!({"EXT_fixture": {"keep": true}});
    }
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let bytes = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .unwrap();
    let serial = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("{prefix}-{}-{serial}.glb", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}
