# Converter ingestion

The Converter is the only workflow allowed to import FBX, OBJ, or Blend. It
invokes Blender headlessly with the same fixed normalization profile as the
desktop page. Other asset workflows never require Blender.

Call `fbx_convert` with an absolute `input` and `output` path. Blender's
executable path is optional when a supported installation is discoverable.
Existing outputs require explicit overwrite authorization. This tool starts the
conversion and writes the requested output.

After completion, inspect the generated GLB with `glb_inspect`. The shared core
re-parses the output before reporting success. A conversion failure preserves an
existing destination. Do not retry by weakening validation or changing the
fixed profile silently.

For delivery normalization, continue with the [GLB workflow](glb.md) on the
generated file. Prefer a separate destination so earlier pipeline stages remain
available. Byte comparisons require the same Blender version and runtime.
