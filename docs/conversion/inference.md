[Home](../README.md) › [Conversion](README.md) › Inference

# Inference

The cache links surprisingly little together. This page covers what it states outright, and how the tools can infer the rest from structure: which sequences animate which NPCs and locations, and which equipment slot an item goes in. Read it to know which links you can rely on and which are educated guesses.

**Status:** sequence candidates are implemented in [`rs2convert::infer`](../../crates/rs2convert/src/infer.rs) and shown in the explorer's [playback bar](../tools/explorer.md#playback). Roles from motion are still to come ([#59](https://github.com/birdbree/rs2ool/issues/59)), and equipment slots aren't inferred yet ([#60](https://github.com/birdbree/rs2ool/issues/60)). Anything inferred is shown as a ranked candidate with its score and evidence, never as a cache fact.

## What the cache states

| Record | Links it states | Links it doesn't |
| --- | --- | --- |
| [NPC](../records/npc.md) | its models; its stand, walk, walk-back and turn sequences | attack, block, death or any other sequence |
| [Location](../records/loc.md) | its models; at most one animation | anything else it might play |
| [Item](../records/obj.md) | inventory model; male and female wear models | which equipment slot it goes in; which body parts it hides |
| [Sequence](../records/seq.md) | its frames (global frame ids); optional hand-item overrides | which models it's meant for |

Everything else has to come from structure.

## Sequences and NPCs

### Frame-base families

A sequence's frames come from [frame packs](../records/frame-pack.md), and every pack carries its own frame base: the transform slots and the vertex labels each one moves. A sequence can only animate a model whose labels fit that base. Packs whose bases are identical form a **frame-base family**, a skeleton shared by every sequence in it. A sequence belongs to the families of the packs its frames come from; in practice that's one family, since only one sequence in the cache mixes bases.

So for an NPC, the sequences in its stand sequence's family are the candidates for everything else it could play.

### The evidence, in order

1. **Family.** Keep the sequences in the stand sequence's family. This never loses a real match: an NPC's walk sequence is in its stand sequence's family for every NPC that has both.
2. **Coverage of the model.** Keep sequences that move at least half of the NPC's model labels. This trims candidates without losing walk sequences. The opposite test (most of the labels a sequence moves must be on the model) doesn't work: a shared base lists labels for every model that uses it, so even an NPC's own stand sequence fails it.
3. **Id distance.** Rank what's left by how far its id is from the stand sequence's. Sequences made for one creature tend to sit at neighbouring ids.
4. **Role from motion.** A death sequence doesn't loop and ends collapsed towards the ground. An attack is short, swings one limb out and comes back to its first pose. A block is a small recoil. This labels candidates rather than ranking them.
5. **Tie-breakers.** Shared frame packs, hand-item overrides (a player or humanoid weapon animation) and the precedence fields.

Posing the model through each candidate and scoring how badly it tears is possible with the [reference deformation](../client/animation.md), but the measurements below suggest it adds little.

### How it's implemented

`SequenceIndex::build` decodes every frame pack and sequence once and records, for each sequence, its families and the labels its frames move. A label counts as moved when a translate, rotate or scale slot on it does something: a non-zero offset or angle, or a scale other than 128. Families whose bases are a prefix of one another count as compatible.

`SequenceIndex::candidates(named, labels)` takes the sequences a model is known to play and its vertex labels. It returns the other sequences in a compatible family that move at least half of those labels (`MIN_COVERAGE`), best first, each with its evidence:

| Field | Meaning |
| --- | --- |
| `score` | `coverage × 8 / (8 + distance)`, 0–1: only comparable within one model's list |
| `coverage` | the share of the model's labels the sequence moves |
| `distance` | how far its id is from the nearest named sequence |
| `pool` | how many sequences share the named sequences' family group; over 300 (`SHARED_POOL`) the candidate is from a shared pool, where the ranking means little |

On the test cache, anchored on each NPC's stand sequence alone, its walk sequence ranks first for 214 of 802 creatures, in the top 5 for 687 (86%) and in the top 20 for 793. In the shared humanoid pool it's in the top 5 for only 73 of 1216, as expected. `crates/rs2convert/tests/infer_sequences.rs` checks this, requiring a top-5 rate of at least 70% for creatures.

### What the measurement shows

From `tests/infer_frame_bases.rs` on the test cache:

| Measure | Result |
| --- | --- |
| Frame packs / distinct frame bases | 1044 / 761 |
| Sequences in the largest family (the shared humanoid skeleton) | 1205 |
| Families with one sequence / two | 347 / 133 |
| Bases that are a strict prefix of another base | 86 |
| NPCs whose walk sequence shares the stand sequence's family | 3401 of 3401 |
| NPCs in the humanoid family | 1745 of 3673 |
| Candidates for the other NPCs | median 10, p90 41 |
| Other NPCs' walk sequence by id distance: nearest / top 5 / top 20 | 588 / 1362 / 1713 of 1734 |
| Humanoid NPCs' walk sequence in the top 20 by id distance | 178 of 1667 |

So the problem splits in two:

- **Creatures.** Family, coverage and id distance give a short, well-ordered list for about half of all NPCs. The many tiny families are partly the same skeleton with fewer slots (the prefix bases), and treating a base as compatible with its extensions would merge them.
- **Humanoids.** Every humanoid fits every humanoid sequence, so structure can't say which attack a given humanoid uses; in the original game that depends on its weapon, which the cache doesn't record. The honest output is the shared humanoid pool, filtered by role and hand-item overrides.

Locations work the same way: their named animation fixes a family, and other candidates come from frame-base compatibility with their model's labels. The explorer doesn't offer location candidates yet.

## Equipment slots

Item definitions have no slot field, which is why the explorer's [player kit](../tools/explorer.md#the-player-kit) makes you put each item in its slot yourself. The evidence:

1. **Labels to body regions.** Wear models are built on the player's skeleton, so their vertex labels say where they attach. [Rig inference](rig-inference.md#naming) already maps labels to body regions. Head labels mean a helmet; neck an amulet; the back of the upper torso a cape; torso and arms a body; legs; hands gloves; feet boots. Right-hand labels alone mean a weapon, left-hand alone a shield, both hands a two-handed weapon.
2. **Overlap with the default body.** Comparing a wear model with each default [identity-kit](../records/idk.md) part confirms the slot, and says which kit parts the item covers, which is what hiding body parts needs.
3. **Options and fields.** "Wield" against "Wear"; an item that can be worn but has no wear model is probably a ring; one that stacks and can be wielded is probably ammunition.
4. **NPC outfits.** Humanoid NPCs reuse item wear models in their model lists. Looking those up in reverse confirms slots (an NPC never wears two helmets) and gives each NPC's outfit.

## Measuring it yourself

The measurement is an ignored test, so it never runs by accident:

```text
cargo test --test infer_frame_bases -- --ignored --nocapture
```

It needs `RS2_CACHE` and prints the figures above. Nothing it reads is written anywhere.

## See also

- [Frame pack](../records/frame-pack.md): frame bases, slots and labels.
- [Seq](../records/seq.md): sequences and their frames.
- [NPC](../records/npc.md) and [Obj](../records/obj.md): the links the cache does state.
- [Rig inference](rig-inference.md): bones from frame-base label sets, and the label-to-region table.
- [Creatures](creatures.md): which sequences conversion collects today.
- [Testing](../development/testing.md): the measurement among the other tests.
