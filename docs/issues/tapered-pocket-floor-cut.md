# Tapered cut on down from a pocket floor with rounded corners is refused (was: old floor kept as a zero-thickness skin)

## Summary

A pocket's floor whose corners were rounded by fillets on **slanted** (tapered) corner edges cannot be cut further down **with a taper**. The plain cut (no taper) is fixed on branch `claude/extrude-cutout-fix-7h10jd` (commit 479ea28c2). The tapered cut is still not.

Before that branch, both variants "succeeded" with a `Boolean retry` warning. The old floor at z = 25 stayed in the body as a zero-thickness skin over a sealed void. The volume was right, but from above the pocket looked as deep as before.

## Model

1. Block 80 × 80 × 40.
2. Pocket 60 × 60 (10 mm in from the edges), cut 15 mm deep from the top with taper −10°. The floor at z = 25 is about 54.7 mm across.
3. Fillet R3 on the 4 rim edges (z = 40) and the 4 slanted corner edges of the pocket.
4. Extrude the pocket floor face (z = 25) 5 mm down as a **cut**, with taper −10°, so the walls carry on without a step.

## Reproduce

Save this as `repro.json` and run `solidcraft-cli run repro.json`:

```json
{"commands": [
  {"command": "sketch.create", "params": {"plane": "XY", "offset": 0, "name": "Block"}},
  {"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [80, 80]}},
  {"command": "sketch.finish", "params": {}},
  {"command": "solid.extrude", "params": {"sketch": "Block", "distance": 40, "body_name": "Box"}},
  {"command": "sketch.create", "params": {"plane": "XY", "offset": 40, "name": "Pocket"}},
  {"command": "sketch.rectangle.two_point", "params": {"p0": [10, 10], "p1": [70, 70]}},
  {"command": "sketch.finish", "params": {}},
  {"command": "solid.extrude", "params": {"sketch": "Pocket", "distance": -15, "taper": "-10 deg", "operation": "cut", "targets": ["Box"]}},
  {"command": "solid.fillet", "params": {"edges": [[40, 10, 40], [70, 40, 40], [40, 70, 40], [10, 40, 40], [11.322452, 11.322452, 32.5], [68.677548, 11.322452, 32.5], [68.677548, 68.677548, 32.5], [11.322452, 68.677548, 32.5]], "radius": 3}},
  {"command": "solid.extrude", "params": {"face": [40, 40, 25], "distance": -5, "operation": "cut", "targets": ["Box"], "taper": "-10 deg"}}
]}
```

Without `"taper"` in the last command it is the plain case. That case is now covered by `examples/regressions/tapered_pocket_floor_cut_after_corner_rounds.json` and the engine test `a_tapered_pocket_floor_cut_on_down_after_corner_rounds_leaves_no_skin`.

In the app: make the block, the pocket (Extrude, Cut, Taper Angle −10°) and the fillets, then select the pocket floor, Extrude, Cut, −5 mm, Taper Angle −10°.

## Expected

One shell. The floor ends up at z = 20 and the pocket walls carry on at −10° for 5 mm more, with the corner rounds' section following. No face is left at z = 25.

## Actual

On `main` (0.22.0, a1b9ca13e), the step succeeds with `Boolean retry: tool displacement 2.415669e-4 mm, rotation 2.000000e-6 rad`. `model.faces` lists both `F4:bottom` (z = 25, normal +Z) and `F7:start` (z = 25, normal −Z). The body has 2 shells: the old floor sits as a skin over a sealed void.

On branch `claude/extrude-cutout-fix-7h10jd`, the result is refused, which beats a wrong result, but the cut cannot be made:

```
error: step 10 (solid.extrude): the cut could not be computed reliably: it would leave a sealed void under a skin of no thickness, at (12.748, 14.861, 24.996). The result was refused rather than kept wrong; ... (tool displacement 2.415669e-4 mm, rotation 2.000000e-6 rad)
```

Only the fillets on the slanted corner edges trigger it. With no fillets, or rim fillets only, both variants work.

## Cause (as far as traced)

- The floor's corner edges are the curves where the corner rounds (cylinders about slanted axes) meet the floor plane. Projected into the face sketch they become fitted conics (`k1…k16`), a hair off the body's edges.
- The plain cut now takes the face itself swept back as its tool (`whole_face::cut_tool`, all walls cutting). It carves it out along the face (`coplanar::carve`) instead of intersecting.
- The tapered cut goes through `extrude_tapered_between` in `doc::eval` with the fitted region. Its tool's cap lies on the floor with walls a hair off the curved edges, so truck's boolean fails and only the nudged retry finds a result, the skin.

A fix probably needs a tapered sweep of the face's own edges, which are exact, so the carve applies to the tapered tool too.

## Note

Upstream (`tangency-labs/solidcraft`) has paused kernel work (#3992 there). Related upstream issues: #2040 (extrude from a face rounded by fillets), #2360 (cut whose profile edges coincide with wall faces), #1183 (tapered pockets on a plate with corner fillets).
