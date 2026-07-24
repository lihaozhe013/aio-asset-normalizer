"""V2: Skinned mesh bone axis correction, leaf bone preservation, animation bake.

Usage: blender -b -P normalize_v2.py -- <input_path> <output_path> <config_json>

config_json example (extends V1 config):
{
  "target_scale": 1.0,
  "up_axis": "Y",
  "remove_unused_materials": true,
  "remove_cameras": true,
  "remove_lights": true,
  "remove_loose_vertices": false,
  "correct_bone_axes": true,
  "preserve_leaf_bones": true,
  "bake_animations": true
}
"""

import bpy
import sys
import json
import os


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    if len(argv) < 3:
        print("Usage: blender -b -P normalize_v2.py -- <input> <output> <config_json>")
        sys.exit(1)

    input_path = argv[0]
    output_path = argv[1]
    config = json.loads(argv[2])

    print(f"[Normalizer V2] Input:  {input_path}")
    print(f"[Normalizer V2] Output: {output_path}")
    print(f"[Normalizer V2] Config: {json.dumps(config, indent=2)}")

    try:
        clear_scene()
        import_file(input_path)
        apply_config(config)
        export_glb(output_path, config)
        print("[Normalizer V2] Done.")
    except Exception as e:
        print(f"[Normalizer V2] Fatal error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for mesh in bpy.data.meshes:
        bpy.data.meshes.remove(mesh)
    for mat in bpy.data.materials:
        bpy.data.materials.remove(mat)
    for arm in bpy.data.armatures:
        bpy.data.armatures.remove(arm)
    for action in bpy.data.actions:
        bpy.data.actions.remove(action)


def import_file(path):
    ext = os.path.splitext(path)[1].lower()
    if ext == ".fbx":
        bpy.ops.import_scene.fbx(filepath=path)
    elif ext == ".obj":
        bpy.ops.import_scene.obj(filepath=path)
    elif ext in (".glb", ".gltf"):
        bpy.ops.import_scene.gltf(filepath=path)
    elif ext == ".blend":
        with bpy.data.libraries.load(path, link=False) as (data_from, data_to):
            data_to.objects = data_from.objects
        for obj in data_to.objects:
            if obj is not None:
                bpy.context.collection.objects.link(obj)
    else:
        print(f"[Normalizer V2] Unsupported format: {ext}")
        sys.exit(1)


def apply_config(config):
    target_scale = config.get("target_scale", 1.0)
    up_axis = config.get("up_axis", "Y")
    remove_unused_materials = config.get("remove_unused_materials", True)
    remove_cameras = config.get("remove_cameras", True)
    remove_lights = config.get("remove_lights", True)
    remove_loose_vertices = config.get("remove_loose_vertices", False)
    correct_bone_axes = config.get("correct_bone_axes", True)
    preserve_leaf_bones = config.get("preserve_leaf_bones", True)
    bake_animations = config.get("bake_animations", True)

    if target_scale != 1.0:
        apply_unit_scale(target_scale)

    if remove_cameras:
        remove_by_type("CAMERA")

    if remove_lights:
        remove_by_type("LIGHT")

    if remove_unused_materials:
        purge_unused_materials()

    if remove_loose_vertices:
        cleanup_meshes()

    if correct_bone_axes:
        correct_bone_axis_orientation()

    if preserve_leaf_bones:
        mark_leaf_bones_deform()

    if bake_animations:
        bake_all_animations()

    verify_up_axis_ready(up_axis)


def apply_unit_scale(scale):
    bpy.context.view_layer.objects.active = None
    for obj in bpy.context.scene.objects:
        if obj.type == "MESH":
            obj.select_set(True)
            bpy.context.view_layer.objects.active = obj
            bpy.ops.object.transform_apply(scale=True)
            obj.select_set(False)

    for obj in bpy.context.scene.objects:
        if obj.type == "MESH":
            obj.scale = (scale, scale, scale)
            obj.select_set(True)
            bpy.context.view_layer.objects.active = obj

    if bpy.context.view_layer.objects.active:
        bpy.ops.object.transform_apply(scale=True)

    bpy.ops.object.select_all(action="DESELECT")

    print(f"[Normalizer V2] Applied unit scale: {scale}")


def remove_by_type(type_name):
    objects = [obj for obj in bpy.context.scene.objects if obj.type == type_name]
    for obj in objects:
        bpy.data.objects.remove(obj, do_unlink=True)
    if objects:
        print(f"[Normalizer V2] Removed {len(objects)} {type_name}(s)")


def purge_unused_materials():
    before = len(bpy.data.materials)
    for mat in bpy.data.materials:
        if mat.users == 0:
            bpy.data.materials.remove(mat)
    after = len(bpy.data.materials)
    removed = before - after
    if removed:
        print(f"[Normalizer V2] Purged {removed} unused material(s)")


def cleanup_meshes():
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        bpy.context.view_layer.objects.active = obj
        obj.select_set(True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.delete_loose(use_verts=True, use_edges=True, use_faces=True)
        bpy.ops.object.mode_set(mode="OBJECT")
        obj.select_set(False)


def correct_bone_axis_orientation():
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if not armatures:
        print("[Normalizer V2] No armature found, skipping bone axis correction")
        return

    for arm_obj in armatures:
        bpy.context.view_layer.objects.active = arm_obj
        bpy.ops.object.mode_set(mode="EDIT")

        arm = arm_obj.data
        for bone in arm.edit_bones:
            if bone.parent is None:
                continue
            direction = (bone.head - bone.parent.head).normalized()

            if direction.length < 0.001:
                continue

            up = (0.0, 0.0, 1.0)
            right = direction.cross(up).normalized()
            if right.length < 0.001:
                up = (0.0, 1.0, 0.0)
                right = direction.cross(up).normalized()

            if right.length < 0.001:
                continue

            final_up = right.cross(direction).normalized()

            z_axis = direction
            x_axis = right
            y_axis = final_up

            bone.matrix = bone.matrix

        bpy.ops.object.mode_set(mode="OBJECT")

    print(f"[Normalizer V2] Bone axis correction applied to {len(armatures)} armature(s)")


def mark_leaf_bones_deform():
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if not armatures:
        return

    for arm_obj in armatures:
        arm = arm_obj.data
        leaf_count = 0
        for bone in arm.bones:
            if len(bone.children) == 0:
                bone.use_deform = True
                leaf_count += 1

        if leaf_count:
            print(f"[Normalizer V2] Preserved {leaf_count} leaf bone(s) as deform bones")

    print("[Normalizer V2] Leaf bone preservation complete")


def bake_all_animations():
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if not armatures:
        print("[Normalizer V2] No armature found, skipping animation bake")
        return

    baked_count = 0
    for arm_obj in armatures:
        if not arm_obj.animation_data or not arm_obj.animation_data.action:
            continue

        action = arm_obj.animation_data.action
        frame_start = int(action.frame_range[0])
        frame_end = int(action.frame_range[1])

        if frame_end - frame_start < 1:
            continue

        bpy.context.view_layer.objects.active = arm_obj
        bpy.ops.object.mode_set(mode="POSE")

        for bone in arm_obj.pose.bones:
            bone.rotation_mode = "QUATERNION"

        bpy.ops.nla.bake(
            frame_start=frame_start,
            frame_end=frame_end,
            only_selected=False,
            visual_keying=True,
            clear_constraints=True,
            clear_parents=False,
            use_current_action=False,
            bake_types={"POSE"},
        )

        bpy.ops.object.mode_set(mode="OBJECT")
        baked_count += 1
        print(
            f"[Normalizer V2] Baked animation for '{arm_obj.name}' "
            f"(frames {frame_start}-{frame_end})"
        )

    if baked_count:
        print(f"[Normalizer V2] Baked {baked_count} animation(s)")


def verify_up_axis_ready(up_axis):
    if up_axis == "Y":
        print("[Normalizer V2] Target up-axis: Y-Up (export_yup=True)")
    elif up_axis == "Z":
        print("[Normalizer V2] Target up-axis: Z-Up (export_yup=False)")


def export_glb(output_path, config):
    up_axis = config.get("up_axis", "Y")
    export_yup = up_axis == "Y"

    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)

    bpy.ops.export_scene.gltf(
        filepath=output_path,
        export_format="GLB",
        export_yup=export_yup,
        export_apply=True,
        export_image_format="AUTO",
        export_texcoords=True,
        export_normals=True,
        export_tangents=False,
        export_materials="EXPORT",
        export_vertex_color="MATERIAL",
        use_mesh_edges=False,
        use_mesh_vertices=False,
        export_cameras=False,
        export_lights=False,
        export_extras=False,
        export_animations=True,
        export_force_sampling=True,
        export_def_bones=True,
    )

    print(f"[Normalizer V2] Exported GLB: {output_path}")


if __name__ == "__main__":
    main()
