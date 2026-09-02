use super::*;

const SAMPLE: &str = "HIERARCHY\nROOT Hips\n{\nOFFSET 0 0 0\nCHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\nJOINT Chest\n{\nOFFSET 0 1 0\nCHANNELS 3 Zrotation Xrotation Yrotation\nEnd Site\n{\nOFFSET 0 1 0\n}\n}\n}\nMOTION\nFrames: 2\nFrame Time: 0.0333333\n0 0 0 0 0 0 0 0 0\n1 2 3 4 5 6 7 8 9\n";

#[test]
fn parses_and_trims_generic_hierarchy() {
    let mut document = BvhDocument::parse(SAMPLE).unwrap();
    assert_eq!(document.joints.len(), 2);
    assert_eq!(document.duration(), 0.0333333);
    document.trim(0.0, 0.0333333).unwrap();
    assert_eq!(document.frames.len(), 2);
}

#[test]
fn authored_rest_pose_is_independent_from_first_motion_frame() {
    let document = BvhDocument::parse(
        "HIERARCHY\nROOT Root\n{\nOFFSET 0 1 0\nCHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\nJOINT Child\n{\nOFFSET 0 2 0\nCHANNELS 3 Zrotation Xrotation Yrotation\nEnd Site\n{\nOFFSET 0 1 0\n}\n}\n}\nMOTION\nFrames: 2\nFrame Time: 0.1\n5 6 7 90 0 0 0 0 0\n5 6 7 90 0 0 0 0 0\n",
    )
    .unwrap();
    let rest = document.rest_transforms_for_retarget().unwrap();
    let frame = document
        .frame_transforms_for_retarget(&document.frames[0])
        .unwrap();
    assert_eq!(rest[0].0, [0.0, 1.0, 0.0]);
    assert_eq!(rest[1].0, [0.0, 3.0, 0.0]);
    assert_eq!(frame[0].0, [5.0, 7.0, 7.0]);
    assert_ne!(rest[0].0, frame[0].0);
    assert_eq!(document.joints[1].end_site, Some([0.0, 1.0, 0.0]));
}

#[test]
fn retargets_rotation_deltas_to_a_mapped_skin() {
    let document = BvhDocument::parse(SAMPLE).unwrap();
    let mapping = MappingFile {
        schema_version: 1,
        source: MappingSource {
            up_axis: "Y".to_owned(),
            forward_axis: "-Z".to_owned(),
            unit: "m".to_owned(),
            root: "Hips".to_owned(),
        },
        target: MappingTarget {
            skin: "CharacterSkin".to_owned(),
            root: "Hips".to_owned(),
        },
        bones: vec![
            BoneMapping {
                source_joint: "Hips".to_owned(),
                target_node: "Hips".to_owned(),
                rotation_offset_xyzw: identity_quaternion(),
            },
            BoneMapping {
                source_joint: "Chest".to_owned(),
                target_node: "Chest".to_owned(),
                rotation_offset_xyzw: identity_quaternion(),
            },
        ],
    };
    let target = SkinData {
        index: 0,
        name: "CharacterSkin".to_owned(),
        skeleton: None,
        joints: vec![0, 1],
        mesh_nodes: Vec::new(),
        nodes: vec![
            super::super::glb::SkinNode {
                index: 0,
                name: "Hips".to_owned(),
                parent: None,
                translation: [0.0, 0.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
            super::super::glb::SkinNode {
                index: 1,
                name: "Chest".to_owned(),
                parent: Some(0),
                translation: [0.0, 1.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
        ],
    };
    let clip = document.retarget_to_skin(&mapping, &target).unwrap();
    assert_eq!(clip.times.len(), 2);
    assert_eq!(clip.channels.len(), 2);
    assert!(clip
        .channels
        .iter()
        .all(|channel| channel.rotations.len() == 2));
}

#[test]
fn mapping_suggestions_normalize_common_rig_prefixes() {
    let document = BvhDocument::parse(SAMPLE).unwrap();
    let target = SkinData {
        index: 0,
        name: "CharacterSkin".to_owned(),
        skeleton: None,
        joints: vec![0, 1],
        mesh_nodes: Vec::new(),
        nodes: vec![
            super::super::glb::SkinNode {
                index: 0,
                name: "mixamorig:Hips".to_owned(),
                parent: None,
                translation: [0.0, 0.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
            super::super::glb::SkinNode {
                index: 1,
                name: "mixamorig:Chest".to_owned(),
                parent: Some(0),
                translation: [0.0, 1.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
        ],
    };
    let suggestions = document.suggest_mapping(&target);
    assert_eq!(suggestions.len(), 2);
    assert!(suggestions.iter().all(|suggestion| {
        suggestion.confidence == SuggestionConfidence::Normalized
    }));
}

#[test]
fn mapping_report_rejects_unknown_and_duplicate_targets() {
    let document = BvhDocument::parse(SAMPLE).unwrap();
    let target = SkinData {
        index: 0,
        name: "CharacterSkin".to_owned(),
        skeleton: None,
        joints: vec![0, 1],
        mesh_nodes: Vec::new(),
        nodes: vec![
            super::super::glb::SkinNode {
                index: 0,
                name: "Hips".to_owned(),
                parent: None,
                translation: [0.0, 0.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
            super::super::glb::SkinNode {
                index: 1,
                name: "Chest".to_owned(),
                parent: Some(0),
                translation: [0.0, 1.0, 0.0],
                rotation: identity_quaternion(),
                scale: [1.0, 1.0, 1.0],
            },
        ],
    };
    let mapping = MappingFile {
        schema_version: 1,
        source: MappingSource {
            up_axis: "Y".to_owned(),
            forward_axis: "-Z".to_owned(),
            unit: "m".to_owned(),
            root: "Hips".to_owned(),
        },
        target: MappingTarget {
            skin: "CharacterSkin".to_owned(),
            root: "Hips".to_owned(),
        },
        bones: vec![
            BoneMapping {
                source_joint: "Hips".to_owned(),
                target_node: "Chest".to_owned(),
                rotation_offset_xyzw: identity_quaternion(),
            },
            BoneMapping {
                source_joint: "Chest".to_owned(),
                target_node: "Chest".to_owned(),
                rotation_offset_xyzw: identity_quaternion(),
            },
            BoneMapping {
                source_joint: "Unknown".to_owned(),
                target_node: "Missing".to_owned(),
                rotation_offset_xyzw: identity_quaternion(),
            },
        ],
    };
    let report = document.mapping_report(&mapping, &target);
    assert!(!report.is_valid());
    assert_eq!(report.duplicate_target_nodes, vec!["Chest"]);
    assert_eq!(report.unknown_source_joints, vec!["Unknown"]);
    assert_eq!(report.unknown_target_nodes, vec!["Missing"]);
}

#[test]
fn key_reduction_removes_a_redundant_middle_key() {
    let mut clip = RetargetClip {
        name: "Test".to_owned(),
        times: vec![0.0, 1.0, 2.0],
        channels: vec![AnimationChannelData {
            node: 0,
            rotations: vec![identity_quaternion(); 3],
            translations: Some(vec![
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [2.0, 2.0, 2.0],
            ]),
        }],
    };
    assert_eq!(clip.reduce_keys(0.0001).unwrap(), 1);
    assert_eq!(clip.times, vec![0.0, 2.0]);
    assert_eq!(clip.channels[0].translations.as_ref().unwrap().len(), 2);
}
