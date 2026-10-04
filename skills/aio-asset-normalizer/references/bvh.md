# BVH processing

Run `bvh inspect <BVH>...` to read hierarchy, joint channels, frame count, duration,
and frame timing. No fixed rig, joint count, device protocol, or IMU mapping is
assumed. A BVH hierarchy's offsets define its authored rest pose; the first motion
frame is not a substitute for that pose.

Trim through the shared core:

```text
bvh process <INPUT_BVH> --output <OUTPUT_BVH> --trim <START_SECONDS> <END_SECONDS> --dry-run
```

After successful dry-run, repeat without `--dry-run`. Omit `--trim` for a checked
identity re-export. Query `schema --command bvh.process` for the shared request.
A minimal job is:

```json
{
  "input": "/absolute/motion/walk.bvh",
  "output": "/absolute/output/walk.bvh"
}
```

Add `"trim": [0, 1]` to retain a time range, and run
`bvh process --job <job.json> --dry-run` followed by execution without dry-run.
Do not combine the job with positional input or other task flags.

Inspect the output and confirm the expected frame count and duration. Input BVH
files cannot be overwritten. Existing outputs require `--overwrite`.

BVH files do not establish physical units or world axes reliably. Inspection and
trimming do not require guessing units. For motion transfer, obtain source axes
and units from the user or trustworthy project metadata, then use the
[retarget workflow](retarget.md). The CLI's BVH `cm`, `Y`, `-Z` defaults are defaults,
not detected metadata.
