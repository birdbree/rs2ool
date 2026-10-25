# TODO

Rough edges, gaps against the original client, and features we don't have yet, roughly in order of how much they'd matter. Each one is a GitHub issue; the issue has the details and is where the discussion goes.

## Bugs and rough edges

- [ ] [#6](https://github.com/birdbree/rs2ool/issues/6) 65535 isn't always "none" in config decoders
- [ ] [#7](https://github.com/birdbree/rs2ool/issues/7) Clip record ids can collide
- [ ] [#8](https://github.com/birdbree/rs2ool/issues/8) Merging models wraps texture triangle indices past 63
- [ ] [#9](https://github.com/birdbree/rs2ool/issues/9) Kit and palette encoders don't check their counts
- [ ] [#10](https://github.com/birdbree/rs2ool/issues/10) Pack reader trusts the header and an unsorted index
- [x] [#11](https://github.com/birdbree/rs2ool/issues/11) Pack names don't cover the manifest
- [ ] [#12](https://github.com/birdbree/rs2ool/issues/12) Loose directories accept several spellings of one record id
- [ ] [#13](https://github.com/birdbree/rs2ool/issues/13) Sidecar `crc32` means different things
- [ ] [#14](https://github.com/birdbree/rs2ool/issues/14) Missing font entry reports "archive has no index.dat"
- [ ] [#15](https://github.com/birdbree/rs2ool/issues/15) `LocDef::flat_shading` is probably named backwards
- [ ] [#16](https://github.com/birdbree/rs2ool/issues/16) A sequence with no frames stays empty
- [ ] [#17](https://github.com/birdbree/rs2ool/issues/17) Sprite ids include entries that aren't sprites
- [ ] [#18](https://github.com/birdbree/rs2ool/issues/18) Font column rule sums palette indices as unsigned
- [ ] [#19](https://github.com/birdbree/rs2ool/issues/19) `--meta-only --xref` rebuilds cross-references every run
- [ ] [#20](https://github.com/birdbree/rs2ool/issues/20) `rs2ool pack sprites --all` is accepted and ignored
- [x] [#21](https://github.com/birdbree/rs2ool/issues/21) Terrain colours disagree between views
- [ ] [#22](https://github.com/birdbree/rs2ool/issues/22) Classic renderer: last column of clipped faces isn't drawn
- [ ] [#23](https://github.com/birdbree/rs2ool/issues/23) Check engine lighting in the explorer after the normals fix
- [x] [#46](https://github.com/birdbree/rs2ool/issues/46) Commands panic when stdout is closed early
- [x] [#57](https://github.com/birdbree/rs2ool/issues/57) Classic view paints lower planes over upper-plane ground

## Fidelity gaps

- [ ] [#24](https://github.com/birdbree/rs2ool/issues/24) Mix secondary frames through interleave slots
- [ ] [#25](https://github.com/birdbree/rs2ool/issues/25) Carry scale transforms into clips
- [ ] [#26](https://github.com/birdbree/rs2ool/issues/26) Apply alpha transforms
- [x] [#27](https://github.com/birdbree/rs2ool/issues/27) Draw terrain the way the original client does
- [x] [#28](https://github.com/birdbree/rs2ool/issues/28) Draw overlay shapes and rotations
- [x] [#29](https://github.com/birdbree/rs2ool/issues/29) Stitch neighbouring regions
- [ ] [#30](https://github.com/birdbree/rs2ool/issues/30) Draw interfaces with the cache's fonts and models
- [ ] [#31](https://github.com/birdbree/rs2ool/issues/31) Pixel-exact check against the original renderer

## Features

- [x] [#32](https://github.com/birdbree/rs2ool/issues/32) Add CI
- [ ] [#33](https://github.com/birdbree/rs2ool/issues/33) glTF export
- [x] [#34](https://github.com/birdbree/rs2ool/issues/34) Render to PNG from the command line
- [x] [#35](https://github.com/birdbree/rs2ool/issues/35) Item packs
- [x] [#36](https://github.com/birdbree/rs2ool/issues/36) Convert spot animations
- [x] [#37](https://github.com/birdbree/rs2ool/issues/37) Bone overlay in the explorer
- [x] [#38](https://github.com/birdbree/rs2ool/issues/38) Evaluate morph tables in the explorer
- [ ] [#39](https://github.com/birdbree/rs2ool/issues/39) Search every record type at once
- [x] [#40](https://github.com/birdbree/rs2ool/issues/40) Female default body for the player kit
- [ ] [#41](https://github.com/birdbree/rs2ool/issues/41) Compare two caches
- [ ] [#55](https://github.com/birdbree/rs2ool/issues/55) Fly the camera across the whole map
- [x] [#59](https://github.com/birdbree/rs2ool/issues/59) Infer which sequences belong to which NPCs and objects
- [x] [#60](https://github.com/birdbree/rs2ool/issues/60) Infer an item's equipment slot from its wear models
- [x] [#65](https://github.com/birdbree/rs2ool/issues/65) Pick humanoid candidate sequences by role, not id distance
- [x] [#67](https://github.com/birdbree/rs2ool/issues/67) Converted NPCs don't play their inferred sequences in compare view
- [x] [#68](https://github.com/birdbree/rs2ool/issues/68) Label each "Open as source" button with what it opens
- [x] [#69](https://github.com/birdbree/rs2ool/issues/69) Show inferred bones on cache models too
- [x] [#70](https://github.com/birdbree/rs2ool/issues/70) Tell deaths apart from jumps, falls and kneels
- [ ] [#72](https://github.com/birdbree/rs2ool/issues/72) Opt-in captioning of candidate sequences with a local vision model, to train a role classifier
- [x] [#61](https://github.com/birdbree/rs2ool/issues/61) Spike: how far do frame bases alone narrow an NPC's sequences?

## Housekeeping

- [ ] [#42](https://github.com/birdbree/rs2ool/issues/42) Rename the step-numbered test files
- [ ] [#43](https://github.com/birdbree/rs2ool/issues/43) Use insta or drop it
