# BVH processing

Call `bvh_inspect` with an absolute `input` path to read the hierarchy, joint
channels, frame count, duration, and timing. No fixed rig, joint count, device
protocol, or IMU mapping is assumed. A BVH hierarchy's offsets define its
authored rest pose; the first motion frame is not a substitute for that pose.

Call `bvh_process` with absolute `input` and `output` paths. Set `trim` to
`[start_seconds, end_seconds]` for a time range or omit it for a checked
identity re-export. Existing output files require explicit overwrite
authorization. Input files cannot be overwritten.

Inspect the written BVH and confirm the expected frame count and duration. BVH
files do not reliably establish physical units or world axes. Inspection and
trimming do not require guessing them. Before motion transfer, obtain source
axes and units from the user or trustworthy project metadata and follow the
[retarget workflow](retarget.md).
