# Particle emitter spec v1

This crate contains the shared renderer-independent emitter contract. It has no
Bevy or Hanabi dependency. Assets are reusable game-folder files at
`assets/particles/<id>.emitter-v1.json`; `id` is 32 lowercase hex digits and
cannot be all zero. `load_asset` checks the document ID against its requested
path. The SHA-256 is computed from the exact loaded bytes.

`EmitterSpec` is closed at every object level, requires every field, and uses
`schema: 1`. `load` reads at most 16 KiB and refuses in this order:
`particle.missing`, `particle.read_failed`, `particle.limit`,
`particle.decode`, `particle.schema`, then `particle.value` with a dotted field
name. Syntax is decoded before schema; schema is checked before typed/closed
decoding; values are validated in struct order. `canonical_bytes` validates and
writes two-space pretty JSON in struct field order, with no negative zero and
one trailing newline. Use it for saves so field order and whitespace do not
vary between editor writes.

## V1 semantics

- `rate_per_second` emits continuously during each `duration_seconds` cycle.
- `burst_count` fires once at every cycle start, including time zero. Set the
  rate to zero for burst-only emitters. Duration must remain positive.
- `looping` repeats the same duration cycle indefinitely. There is no delay or
  overlapping-cycle option.
- V1 uses solid billboards and has no texture reference.
- A point starts at the origin with initial velocity along local +Y.
- A sphere samples positions uniformly in its volume and sends particles
  radially outward; the center uses +Y as its direction.
- Box `size_m` contains full side lengths. A box samples positions uniformly
  in its full-size volume and sends particles radially outward from its center;
  the center uses +Y as its direction.
- A cone samples positions uniformly on a disk of `radius_m` in local XZ at
  the origin. Initial velocity directions are uniform in solid angle within
  `angle_degrees` as a half-angle around local +Y.
- Speed and lifetime are sampled uniformly from their respective min/max
  ranges. Size is the billboard's full width. Size and linear RGBA interpolate
  linearly from start to end by normalized particle age. A particle is dead
  when its age is greater than or equal to its lifetime.
- Shape positions and initial velocity directions are emitter-local; apply
  emitter translation and rotation once at spawn, ignoring scale. Gravity is
  world-space. Spawned particles simulate in world space and do not follow
  later emitter motion.

`max_live_estimate` is an upper bound, not an exact live count:

```text
min(max_particles,
    ceil(rate_per_second * (looping ? max_lifetime : min(max_lifetime, duration)))
      + burst_count * (looping ? ceil(max_lifetime / duration) : 1))
```

The calculation uses saturating integer addition before the configured cap.

## Conservative v1 bounds

All floating-point values must be finite. Inclusive bounds are exposed in
`NUMERIC_BOUNDS` and via `numeric_bound`; cross-field constraints are enforced
by `validate`.

| Field | Bounds |
| --- | --- |
| `name` | 1–96 UTF-8 bytes |
| `emission.rate_per_second` | 0–10,000 |
| `emission.burst_count` | 0–10,000 |
| `emission.duration_seconds` | 0.001–3,600 seconds |
| `lifetime.min_seconds`, `lifetime.max_seconds` | 0.01–300 seconds; min <= max |
| `shape.radius_m`, `shape.size_m` | 0.001–100 metres |
| `shape.angle_degrees` | 0–90 degrees |
| `motion.speed_min_m_per_s`, `motion.speed_max_m_per_s` | 0–1,000; min <= max |
| `motion.gravity_m_per_s2` | each component -100–100 |
| `motion.drag_per_s` | 0–100 |
| `look.size_start_m`, `look.size_end_m` | 0–100 metres |
| `look.color_*_linear_rgba` | RGB 0–16, alpha 0–1 |
| `limits.max_particles` | 1–100,000 |

`default_emitter()` provides the single starting value set for the editor's New
emitter action. Its ID is a valid placeholder; the editor must assign a fresh
stable ID before saving a new asset.
