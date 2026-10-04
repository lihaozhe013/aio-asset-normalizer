# Converter ingestion

The Converter is the only workflow allowed to import FBX, OBJ, or Blend. It invokes
Blender headlessly with the same fixed normalization profile as the desktop page.
The other asset workflows never require Blender.

```text
fbx convert <INPUT>... --blender <BLENDER_EXECUTABLE> --dry-run
```

`--blender` is optional when a supported installation is discoverable. A macOS
`.app` path is accepted. `--out <OUTPUT_GLB>` requires exactly one input; otherwise
outputs are sibling `<stem>_normalized.glb` files. Existing outputs require
`--overwrite`. Query `schema --command fbx.convert` for the underlying per-file
request shape; the CLI uses flags, not `--job`.

Dry-run checks readable input paths, supported extensions, Blender availability,
and destination conflicts. It does not import source contents, launch Blender,
or prove that conversion will succeed.

After successful preflight, execute the authorized conversion without dry-run.
The shared core validates a temporary GLB before atomic submission. Failure keeps
an existing destination intact. Read every entry in `results.files`, even if the
invocation fails overall, and inspect successful outputs using `glb inspect`.

For delivery normalization, proceed with the [GLB workflow](glb.md) on the generated
GLB. Prefer explicit separate destinations to avoid overwriting earlier pipeline
stages. Do not retry conversion by removing validation or changing the fixed
profile silently. Byte comparisons require the same Blender version and runtime.
