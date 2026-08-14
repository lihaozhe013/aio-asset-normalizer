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
        name: "CharacterSkin".to_owned(),
        joints: vec![0, 1],
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
