[Home](../README.md) › [Tools](README.md) › Captions

# Captions

`rs2ool cache caption` draws an NPC's candidate sequences on its model and asks a vision model running on your own machine what each one shows. It keeps the answers in a local file: a short description, plus one class from a fixed list. The captions are cheap and roughly right, and they're quick to check by eye, which makes them a practical way to build a labelled set of sequences. The cache doesn't name its deaths, attacks or jumps, so [role inference](../conversion/inference.md#roles) has to go on motion alone.

Captioning is opt-in and only adds evidence. Candidate inference and the motion roles work the same with or without it. A caption reaches a pack only when you ask for it with `--describe` ([Into the packs](#into-the-packs)).

Implemented in [`rs2ool::caption`](../../src/caption/mod.rs), with the settings in [`rs2ool::settings`](../../src/settings.rs). It needs the `caption` cargo feature, which is on by default.

## Privacy

- **Off until you turn it on.** Nothing is contacted until captioning is enabled in the settings or `--enable` is passed. A dry run never contacts anything.
- **Local by default.** The default endpoint is `http://127.0.0.1:11434`. For an endpoint on another machine, the command warns that images will be sent to it. Plain HTTP is all the default build speaks; https needs the `caption-tls` feature.
- **Nothing downloaded.** The tools don't fetch models or weights. You run the server and choose the model.
- **Nothing committed.** Captions are derived from the cache, so they're kept in your data directory, outside the repository.

## Setting up a server

Any server that speaks Ollama's API or the OpenAI-compatible chat API, and has a model that takes images, will do. For example, with Ollama:

```text
ollama pull <a vision model>
rs2ool cache caption --check --enable --model <the model>
```

With llama.cpp's server, LM Studio or vLLM, choose the OpenAI-compatible API and the server's address:

```text
rs2ool cache caption --check --enable --backend openai --endpoint http://127.0.0.1:8080 --model <the model>
```

`--check` asks the server which models it has and marks the chosen one. It doesn't need a cache.

Choose a model that answers straight away rather than one that thinks first. Requests to Ollama ask for no thinking, but some thinking models reason anyway, for a minute or more per image, and can leave no answer at all; an empty reply counts as a failure and isn't stored. On an 8 GB machine, a 2B instruct model answers in about 7 seconds per image.

## Settings

The settings live in `settings.json` in your platform's config directory, under `caption`. The explorer keeps its output folder in the same file. Every key is optional.

| Key | Default | Meaning |
| --- | --- | --- |
| `enabled` | `false` | whether captioning may contact the server |
| `backend` | `"ollama"` | `"ollama"` (`/api/chat`) or `"openai"` (`/v1/chat/completions`) |
| `endpoint` | `"http://127.0.0.1:11434"` | the server's base URL; an OpenAI-compatible one may end in `/v1` or not |
| `model` | `""` | the model to ask, as the server names it |
| `api_key_env` | none | the environment variable holding an API key, sent as a bearer token; the key itself is never stored |
| `timeout_secs` | `120` | how long to wait for one reply |
| `frames` | `6` | frames per image, 1–16 |
| `prompt` | none | your own question instead of the built-in one; `{frames}` and `{classes}` are filled in |

Command-line flags override them for one run without saving anything: `--enable`, `--backend`, `--endpoint`, `--model`, `--api-key-env`, `--timeout`, `--frames` and `--prompt`.

## The command

```text
rs2ool cache caption --npc 1 [--dry-run] [--limit N] [--seq A,B,…] [--images DIR] [--redo] [SERVER FLAGS]
rs2ool cache caption --all   [the same]
rs2ool cache caption --check [SERVER FLAGS]
rs2ool cache caption --import FILE --model NAME
```

- `--npc N` captions every candidate of NPC `N`, in the order the explorer's picker gives them.
- `--all` captions every candidate sequence of every NPC once, on the first NPC (by id) it's a candidate for. For speed, it doesn't pose every NPC's candidates first, so one that won't pose fails when it's drawn.
- `--dry-run` draws each image and prints its size, the URL and the question, without sending anything. Captioning doesn't have to be on, and no model has to be set.
- `--images DIR` also writes each image as `npc<N>-seq<S>.png`, so you can see what the model is shown.
- `--limit N` stops after `N` images.
- `--seq A,B,…` takes only those candidates.
- `--redo` captions again what's already captioned. Without it, a rerun only does what's missing: with `--npc`, a sequence already captioned on that NPC by the same model, and with `--all`, one already captioned on any NPC.

Each caption prints as it comes back, as `npc 1 seq 2820: [death] Collapses forward and lies flat`. If a sequence fails, the error is printed and the run carries on, then exits 3. Captioning while it's off, or with no model set, is a usage error (exit 1).

## What the model is shown

Each sequence becomes one PNG drawn with the [classic renderer](../crates/rs2classic.md): a grid of up to `frames` frames, left to right and top to bottom, evenly spaced in time from the first frame to the last. A frame held for a long time is shown once. Faces fade as the original client fades them ([Animation](../client/animation.md#alpha)), so a model fading away is seen to.

- All the tiles share one camera, fitted to every frame shown, so a model that falls, shrinks or moves away does so on screen.
- The camera is three-quarters on and tilted slightly down.
- The darker band is the ground at the model's lowest point in the first frame.

Six frames make a 768 × 512 image.

## The question and the classes

The built-in question explains the grid and asks for two lines: what the character does, in under 12 words, then the one class that fits best from this list:

| Class | Gloss given |
| --- | --- |
| `idle` | stays where it is and barely moves |
| `walk` | steps, runs, flies or swims in a repeating cycle, as if travelling |
| `attack` | strikes, bites, kicks, casts or shoots at something ahead, then returns to its starting pose |
| `block` | braces, flinches or recoils as if hit, then returns to its starting pose |
| `gesture` | waves, bows, dances, cheers or makes another expressive movement aimed at no one |
| `work` | repeats a task with a tool or its hands, such as mining, chopping, fishing or cooking |
| `death` | goes down, then squashes flat into the ground and stays there |
| `collapse` | crumples into a heap and stays down, without squashing flat |
| `fall over` | topples backwards, forwards or onto its knees, then gets up or keeps moving |
| `fall away` | moves down below the ground band at full size, out of sight, as into a gap |
| `descend` | comes down slowly from above, floating or lowering itself, and settles on the ground |
| `jump` | pushes off and leaves the ground, leaping up, over or across |
| `land` | arrives from the air and takes the impact, ending crouched or standing on its feet |
| `vanish` | gets smaller or fades in place until nothing is left, as in a teleport |
| `other` | none of these |

Each gloss names the visible cue that sets its class apart, and none uses another class's name, so neither a reply nor the question can be read as naming two classes; a unit test checks that. Each pair most easily confused gets one cue that separates it: a `death` squashes flat, a `collapse` crumples and stays down, and a `fall over` gets up or keeps moving; `fall away` keeps its size as it goes below the ground band, and `vanish` gets smaller or fades in place; `land` comes down fast onto its feet, and `descend` comes down slowly.

Requests ask for temperature 0, no streaming and, from Ollama, no thinking. Small models don't always follow the format, so the reply is read leniently:

- The description is the first line, with list markers, `Line 1:` prefixes, quotes and emphasis stripped.
- The class is taken from the last line that names one: the whole line if it's a class name, or else the longest class name it contains as whole words.
- A reply that names no class is stored without one.

## Where captions go

```text
<data dir>/captions/<cache CRC, 8 hex digits>.json
```

The data directory is the platform's, for example `~/Library/Application Support/rs2ool` on macOS or `~/.local/share/rs2ool` on Linux. There's one file per cache, keyed by the same CRC the packs record (the versionlist's), so captions from different caches don't mix. The file holds `cache_crc` and a list of captions, each with:

| Field | Meaning |
| --- | --- |
| `npc` | the NPC whose model the sequence was drawn on |
| `seq` | the sequence |
| `model` | the vision model that answered |
| `frames` | how many frames the image showed |
| `text` | the description |
| `class` | the class it picked, or `null` |
| `reply` | the whole reply, as it came |
| `review` | once it's checked by eye: `{"right": true}`, or `{"right": false, "class": "death"}` with the right class if the reviewer gave one; absent until then |

Captions are keyed by NPC, sequence and vision model, and written sorted by that key. The file is rewritten through a temporary file after every caption and every review, so stopping a run part way keeps everything captioned so far. Captioning a sequence again with `--redo` replaces its caption, review and all.

## Importing labels

Labels from another labeller, a person or a program, can go into the caption file too:

```text
rs2ool cache caption --import labels.json --model <labeller>
```

The file is a list of `{"npc", "seq", "class", "text"}` objects, plus an optional `"frames"`. Every class has to be one from the list above (or `null`); one that isn't stops the import (exit 1) before anything is written. Each label becomes a caption by `<labeller>`, so it shows in the explorer's picker and can be reviewed like any other. Importing a label again replaces the caption but keeps its review. `--dry-run --images DIR` draws the frame grids such a labeller would look at.

## Into the packs

`rs2ool pack npc … --with-anims --describe` and `pack player-kit --with-anims --describe` write each clip's caption into the anims pack's manifest, as a `descriptions` table ([Descriptions](../pack/anims.md#descriptions)). For each clip they take the best caption of its sequence:

1. a reviewed caption first;
2. then one drawn on this NPC (for the player kit, any NPC);
3. then by labeller name.

A caption marked wrong with no class given is left out. A reviewed caption gives its checked class. The explorer's Convert action describes too, so its packs match the command line's.

## In the explorer

The explorer's **Captions** window holds the same settings with a connection test, and runs the caption job for the selected NPC. Captions show in the sequence picker, and the chosen sequence's caption can be marked right or wrong, or corrected to another class, as it plays. See [Explorer](explorer.md#captions).

## Learning roles from reviews

Reviewed captions are labels. `rs2ool cache classify` learns a small role tree from them and compares it with the hand-tuned motion rules:

```text
rs2ool cache classify [--labels FILE] [--trust MODEL]… [--weak W] [--depth N] [--min-leaf W] [--verbose] [--save]
```

It prints how many labelled sequences each gets right, the families with enough labels to try a tree of their own and how those do, a confusion table for each (labelled role against what it said), and the global tree. `--labels` adds a local labels file (`[{"npc": 1, "seq": 2820, "class": "death"}, …]`, defaulting to `RS2_SEQUENCE_LABELS`), though reviews win. `--trust <model>` (repeatable) counts that model's unreviewed captions as checked labels, scored like reviews; meant for imported labels you've spot-checked and found good. `--weak W` also learns from any other unreviewed captions' own classes, each counting `W`; they're never scored. `--verbose` lists every sequence with its three roles. `--save` keeps the trees, and the explorer's sequence picker then uses them for roles. See [Learning roles from labels](../conversion/inference.md#learning-roles-from-labels) for how it works and how it compares.

## See also

- [Inference](../conversion/inference.md#roles): candidates and the motion roles captions are meant to check.
- [Cache commands](cache-commands.md): the other `cache` subcommands.
- [rs2ool crate](../crates/rs2ool.md): the `caption` and `caption-tls` features.
