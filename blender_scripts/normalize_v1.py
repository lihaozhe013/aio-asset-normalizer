"""V1: Static mesh / material normalization.

Usage: blender -b -P normalize_v1.py -- <input_path> <output_path> <config_json>

config_json example:
{
  "target_scale": 1.0,
  "up_axis": "Y",
  "remove_unused_materials": true,
  "remove_cameras": true,
  "remove_lights": true,
  "remove_loose_vertices": false
}
"""

import bpy
import sys
import json
import os


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    if len(argv) < 3:
        print("Usage: blender -b -P normalize_v1.py -- <input> <output> <config_json>")
        sys.exit(1)

    input_path = argv[0]
    output_path = argv[1]
    config = json.loads(argv[2])

    print(f"[Normalizer] Input:  {input_path}")
    print(f"[Normalizer] Output: {output_path}")
    print(f"[Normalizer] Config: {json.dumps(config, indent=2)}")

    clear_scene()
    import_file(input_path)
    apply_config(config)
    export_glb(output_path, config)

    print("[Normalizer] Done.")


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for mesh in bpy.data.meshes:
        bpy.data.meshes.remove(mesh)
    for mat in bpy.data.materials:
        bpy.data.materials.remove(mat)


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
        print(f"[Normalizer] Unsupported format: {ext}")
        sys.exit(1)


def apply_config(config):
    target_scale = config.get("target_scale", 1.0)
    up_axis = config.get("up_axis", "Y")
    remove_unused_materials = config.get("remove_unused_materials", True)
    remove_cameras = config.get("remove_cameras", True)
    remove_lights = config.get("remove_lights", True)
    remove_loose_vertices = config.get("remove_loose_vertices", False)

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

    print(f"[Normalizer] Applied unit scale: {scale}")


def remove_by_type(type_name):
    objects = [obj for obj in bpy.context.scene.objects if obj.type == type_name]
    for obj in objects:
        bpy.data.objects.remove(obj, do_unlink=True)
    if objects:
        print(f"[Normalizer] Removed {len(objects)} {type_name}(s)")


def purge_unused_materials():
    before = len(bpy.data.materials)
    for mat in bpy.data.materials:
        if mat.users == 0:
            bpy.data.materials.remove(mat)
    after = len(bpy.data.materials)
    removed = before - after
    if removed:
        print(f"[Normalizer] Purged {removed} unused material(s)")


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


def verify_up_axis_ready(up_axis):
    if up_axis == "Y":
        print("[Normalizer] Target up-axis: Y-Up (export_yup=True)")
    elif up_axis == "Z":
        print("[Normalizer] Target up-axis: Z-Up (export_yup=False)")


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
        export_colors=True,
        use_mesh_edges=False,
        use_mesh_vertices=False,
        export_cameras=False,
        export_lights=False,
        export_extras=False,
    )

    print(f"[Normalizer] Exported GLB: {output_path}")


if __name__ == "__main__":
    main()
