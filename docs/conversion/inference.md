[Home](../README.md) › [Conversion](README.md) › Inference

# Inference

The cache links surprisingly little together. This page covers what it states outright, and how the tools can infer the rest from structure: which sequences animate which NPCs and locations, and which equipment slot an item goes in. Read it to know which links you can rely on and which are educated guesses.

**Status:** sequence candidates are implemented in [`rs2convert::infer`](../../crates/rs2convert/src/infer.rs) and shown in the explorer's [playback bar](../tools/explorer.md#playback). Each candidate's role (death, action, down, drop, vanish, idle or other) is worked out from its motion. Equipment slots are inferred from wear models in [`rs2convert::slot`](../../crates/rs2convert/src/slot.rs) and used by the explorer's player kit. Anything inferred is shown as a ranked candidate with its score and evidence, never as a cache fact.

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
4. **Role from motion.** A death ends collapsed towards the ground; an attack or a block moves and comes back to its first pose; an idle barely moves. This labels candidates rather than ranking them (see [Roles](#roles) below).
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

### Roles

`Motion::measure` poses the NPC's model through every frame of a sequence with the reference deformation and measures, as shares of the model's height in the first frame unless noted:

| Measure | Meaning |
| --- | --- |
| `end_height` | the last frame's height |
| `drop` | how far the model's top has dropped from the first frame to the last |
| `base_drop` | how much lower its lowest point ends (negative: higher) |
| `lift` | how far its lowest point rises above where it started, at most |
| `rise` | how far its top rises above where it started, at most |
| `spread` | its widest horizontal extent at the end over at the start (a ratio) |
| `returns` | the mean distance each vertex ends from where it started |
| `peak` | the largest mean distance of the vertices from the first frame, over all frames |
| `hold` | how long the last frame is held, in cycles |
| `duration` | how long one run lasts, in cycles |
| `stretch` | how much the model's faces stretch: in the worst frame, the length 95% of face edges stay within, over their rest length (against the rest pose, unlike the others) |

The first frame is the reference, not the rest pose, because some models are animated in a pose quite unlike their rest pose: measured against the rest pose, a few stand sequences looked like deaths. `Motion::role` then picks the first rule that fits:

| Role | Rule | What it is |
| --- | --- | --- |
| vanish | `end_height < 0.05` | shrinks away, sinks out of sight, a teleport |
| drop | `base_drop ≥ 0.3` | the whole body goes down: a fall, or a jump down into a gap |
| death | `drop ≥ 0.3`, `end_height ≤ 0.75` and `hold ≥ 50` (1 s) | goes down and stays down |
| down | `drop ≥ 0.3` and `end_height ≤ 0.75` | goes down without staying: falls over, kneels, sits, floats down |
| idle | `peak < 0.06` | barely moves |
| action | `returns ≤ 0.15` | moves and comes back: an attack, a block or a gait, which motion alone can't tell apart |
| other | anything else | such as a sequence that ends somewhere new |

The hold is the strongest death signal. Last frames are held either briefly (2–50 cycles) or for a very long time (10,000–20,000), with little in between, and a death holds. Vanish comes first because a model shrinking about its centre also lifts its lowest point and lowers its top, which would otherwise read as a jump or a death.

**Checked against what is named.** Over 250 creatures, their stand sequences come out idle 172 times, action 64 (bobbing or flapping idles), other 8 and drop once. Their walk sequences come out action 201 times, idle 19, other 24 and down once. 120 of the 250 creatures have a death among their candidates, and 125 a death or a vanish (some creatures die by shrinking or sinking away). `crates/rs2convert/tests/infer_sequences.rs` checks these rates. `with_roles` fills in the `role` of a list of candidates for a creature.

**Checked against labels.** 21 sequences labelled by eye (17 from the humanoid pool's former "deaths", 4 from creatures) give, class by role:

| Labelled as | death | down | drop | vanish |
| --- | --- | --- | --- | --- |
| death (3) | **3** | | | |
| fall over (4) | 2 | 2 | | |
| land (1) | 1 | | | |
| descend, floating down (2) | | 2 | | |
| fall away into a gap (3) | | | 1 | 2 |
| jump down (2) | | | 2 | |
| vanish (4) | | 1 | | 3 |
| other (2) | | 2 | | |

Against those labels, the death rule's precision went from 0.14 with the original single rule (`drop ≥ 0.3` and `end_height ≤ 0.75`, which called all 21 deaths) to 0.50, still finding every death. The two falls into a gap that vanish do drop out of sight.

What's left is a real limit of motion alone: falling backward or to the knees and then lying still for good, or landing heavily and staying put, looks exactly like a death. Those sequences may well be used as deaths in some situations. Telling them apart would need more than the motion, such as captions or which sequences the game plays together. `rs2ool cache caption` asks a vision model you run to describe each candidate and pick a class, as labels to check these roles against ([Captions](../tools/captions.md)).

The labels aren't in the repository, since they describe the game's content. `crates/rs2convert/tests/infer_labels.rs` reads them from the JSON file named by `RS2_SEQUENCE_LABELS` (`[{"npc": 1, "seq": 2820, "class": "death", "note": "…"}, …]`), reports the table above and checks the death rule still beats the original. Without the file it skips.

### Learning roles from labels

The rules above were tuned by hand. `rs2ool cache classify` learns roles from sequences labelled by eye instead, and measures how the two compare. Its labels are [captions](../tools/captions.md) reviewed in the explorer, plus any from the local labels file. Each label's class becomes a role:

| Classes | Role |
| --- | --- |
| `idle` | idle |
| `walk`, `attack`, `block`, `gesture`, `work`, `jump` (and `action` from the labels file) | action: all of them move and come back |
| `death`, `collapse` | death: both go down and stay down |
| `fall over`, `descend` | down |
| `fall away` | drop |
| `vanish` | vanish |
| `land`, `other` | other: a landing starts in the air |

Labels [imported](../tools/captions.md#importing-labels) from another labeller count once you pass `--trust <labeller>`; reviews still win over them. Each labelled sequence is posed on its NPC and measured, as above. [`rs2convert::classify`](../../crates/rs2convert/src/classify.rs) grows a decision tree on the eleven measures: binary splits chosen greedily on weighted Gini impurity, at most 4 deep, with at least one label in every leaf. The tree is small enough to read, and the command prints it. It's scored leave-one-out: each sequence's role comes from a tree grown on every label but that one, so it's never marked on something it learned from.

On 633 labelled sequences, the rules got 425 right (67%) and the tree 468 (74%). The labels were 46 reviewed captions, 16 from the labels file, and 571 imported from another labeller, covering one NPC from each of 56 skeleton families plus 100 of the shared humanoid pool. Where the imported labels and the reviews cover the same sequences, they agree on the role 12 times out of 15. Two cautions:

- The depth and leaf size were picked by trying several on earlier labels, which flatters the tree a little.
- Most of the labels weren't checked one by one.

Where they differ:

- **The rules** call 71 actions "other" and 20 "idle", and 16 deaths "other".
- **The tree** gets 332 of the 370 actions, against the rules' 265, and 48 of the 67 deaths, against 42. But it still has too few downs, drops and vanishes to place them well: 3 of 21 downs (the rules get 6), none of 8 drops (the rules get 4) and 5 of 10 vanishes (the rules get 4).

More labelled downs, drops and vanishes are what would help it most.

#### Trees per skeleton family

NPCs on one skeleton family share their sequences, so a family with enough labels might be better served by a tree of its own. A family with at least 20 checked labels over two roles or more (`FAMILY_MIN`) gets one tried, scored leave-one-out within the family. It's kept only if it beats the global tree on that family's sequences:

| Family | Labels | Rules | Global tree | Own tree | |
| --- | --- | --- | --- | --- | --- |
| humanoids | 148 | 94 | 105 | 110 | kept |
| goblins | 24 | 15 | 15 | 19 | kept |
| gnomes | 40 | 27 | 28 | 28 | |
| lesser demons | 26 | 23 | 25 | 24 | |
| ogres | 20 | 15 | 14 | 12 | |
| trolls | 20 | 14 | 17 | 15 | |
| monkeys | 39 | 20 | 26 | 16 | |

With a few dozen labels, a family tree usually does worse than the global one, which has seen ten times as much. Using the kept family trees and the global tree elsewhere gets 477 right (75%), though choosing which to keep on the same labels flatters that a little.

Unreviewed captions' own classes can be added as weak labels (`--weak W`, each counting `W`). With the 2B captioning model they don't help: its classes are mostly wrong.

`--save` writes the trees to `role-tree.json` in the data directory: the global tree, the kept family trees, and the CRC of the cache they were learned on. When the file is there and matches the open cache, the explorer's sequence picker uses the NPC's family tree, or else the global one, to group its candidates, marking each group "learned". Conversion always uses the rules, so packs don't depend on a local file. The trees are only ever learned locally; none is shipped.

### Sequences that tear the model

A candidate shares the NPC's skeleton family, but it may have been made for another model on that skeleton, labelled differently. Played on this model, it tears it apart. The red dragon is the clearest case. Its candidates rear the upper body back about 53° with a transform whose label list leaves out the chest, so the chest stays put while the neck and shoulders swing up. The original client would deform it the same way: the stretch is in the data, not the deformation.

The `stretch` measure catches this. Every vertex has exactly one label, so faces that bridge two labels stretch at every bend, and a few long edges are normal: the single worst edge of the sequences NPCs name reaches 12× at the 99th percentile. So it looks at the 95th-percentile edge in the worst frame instead:

On the sequences NPCs name, played on their own models, that's 1.13 at the median and 1.45 at the 99th percentile; under 1% reach `TEARS`, 1.5 (19 of 2561 on every third NPC). The red dragon's candidates 80, 81, 89 and 91 come out at 1.96–2.36, while its own stand and walk are 1.06 and 1.17.

Creatures are where it matters: 618 of the 4941 candidates on those NPCs tear (12.5%). The shared humanoid pool mostly fits, so across every candidate it's only 1.7%. A candidate that tears (`Candidate::tears`) goes after the clean ones in its role group, and the picker marks it ("stretches 2.4×"). Its role still counts: the motion usually reads clearly even when the mesh tears. `crates/rs2convert/tests/infer_sequences.rs` checks that under 2% of the named sequences reach the threshold and that some creature candidates do.

### Every candidate, grouped by role

There's no cap on the candidates. `npc_candidates` loads every one onto the NPC's model, gives each a role, leaves out any that won't pose, and `sort_by_role` groups them: deaths, then actions, then idles, then others.

- In each group, candidates that tear the model go last.
- **Creatures** keep score order within each group; they have a few dozen candidates at most.
- **Humanoids** get the whole shared pool, about 1200 sequences. Id distance means almost nothing there, so within a group deaths go most collapsed first (largest `drop`), and actions go those that fit in 60 cycles first (1.2 s, `SHORT_ACTION`, since attacks are brief), then the most motion first. The rest keep score order.

Classifying the pool takes about 0.1 s per NPC in a release build, so it's done per NPC rather than cached. On four humanoids with different label layouts, the pool came out as roughly 670–770 actions, 190–310 idles and 150–180 others, with about 20 deaths, 27 downs, 9 drops and 11 vanishes (the former 60-odd "deaths"), and 3 sequences that wouldn't pose. About half the actions fit in 1.2 s. Converting a humanoid with every candidate takes about 4 s in a release build and writes about 1.1 MB of packs.

How well the short-first rule puts attacks ahead of other brief gestures can't be measured from the cache, which names no attacks; it's a reasonable rule, not a checked one. 

### Converting them

`npc_extras(records, id)` returns the ids of an NPC's candidates, to convert alongside its named sequences. `rs2ool pack npc --infer` and the explorer's Convert action both use it, so they write identical packs, and every sequence the explorer's picker offers has a clip in the converted creature. Some candidates, especially from the shared pool, weren't made for the model and re-skin poorly; their clips exceed the [validation](validation.md) threshold, and the command exits with status 3.

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

Item definitions have no slot field. Wear models are built on the player's skeleton, though, so their vertex labels say where they attach. [`rs2convert::slot`](../../crates/rs2convert/src/slot.rs) reads the slot from them.

### Label groups

These come from the wear models themselves and match the default body's [identity-kit](../records/idk.md) parts:

| Labels | Where |
| --- | --- |
| 1, 3, 49 | the head (skull and face) |
| 2, 8 | the neck and upper chest |
| 4 | the waist |
| 9–16 | where a cape hangs from |
| 17–26 | the arms, shoulder to wrist (19 and 22 the forearms) |
| 27, 28 | the right and left hands |
| 29–44 | the legs, hip to ankle |
| 45–48 | the feet |
| 50 and up | the held-object point a weapon hangs from |
| 255 | unassigned vertices, ignored |

### Rules

`slot_for` takes the labels of an item's male wear models (the female ones are built on the same skeleton) and picks the first that fits:

1. any cape label: **cape**;
2. any foot label: **feet**;
3. any leg label: **legs**;
4. both hands and a forearm, but no upper arm: **hands**;
5. any arm label or the waist: **body**;
6. any head label: **head**;
7. nothing but the neck and upper chest: **amulet**;
8. the left hand only: **shield**;
9. the right hand, both hands, or the held-object point: **weapon**.

The order settles overlaps: leg armour often reaches the waist, and a full helm reaches the neck. Alongside the slot, a guess records whether the item has an option that puts it on ("Wear", "Wield" or "Equip"). Many items have wear models but no such option: tools and the like, whose models are only shown in the hand during an animation. Those are flagged as held only. `both_hands` marks a weapon drawn on both hands, like claws. Two-handed weapons that hang from the held-object point, as most do, aren't detected.

`covers` says which default body parts an item covers: each kit part whose own labels the item has at least 80% of. Body armour with sleeves covers the torso and arms; boots cover the feet. That's what hiding body parts under an item needs ([#40](https://github.com/birdbree/rs2ool/issues/40)), but it isn't applied yet. The 80% line is a guess, and some cases need more than labels: a medium helmet and a full helm cover the same head labels, but only one of them hides a beard.

### How well it does

On the test cache, 1586 items have a wear model:

| Slot | Items | Held only |
| --- | --- | --- |
| weapon | 648 | 190 |
| head | 222 | 7 |
| legs | 162 | 6 |
| body | 139 | 7 |
| shield | 133 | 25 |
| cape | 82 | 0 |
| amulet | 79 | 0 |
| feet | 54 | 1 |
| hands | 53 | 0 |

14 items stay unplaced, all with nothing but unassigned vertices. Three checks back the rule up:

- **Hand-checked items:** 57 items across every slot, checked by name, all get the right slot.
- **Body-part overlap:** for the 441 wearable items that cover a default body part, the part they cover points at the same slot 436 times.
- **NPC outfits:** NPCs are often dressed in item wear models. Leaving out identity-kit body-part models (which some items reuse as their wear model), 713 NPCs wear two or more items and only 34 of them would be wearing two in one slot. Most of those 34 are a hat worn with a hair-like model.

Like the sequence labels, the hand-checked list stays local: `crates/rs2convert/tests/infer_slots.rs` reads it from the JSON file `RS2_ITEM_SLOTS` names (`[{"item": 1153, "slot": "head"}, …]`) and skips that part without it.

The explorer's [player kit](../tools/explorer.md#the-player-kit) uses the guesses to put an item in its slot for you. Rings and ammunition have no wear model, so there's nothing to read for them. Captioning ([#72](https://github.com/birdbree/rs2ool/issues/72)) could check slots by eye too.

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
